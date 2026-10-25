use std::{
    collections::HashMap,
    fmt::Display,
    net::SocketAddr,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
        Arc,
    },
};

use deno_core::{
    v8::{self, Global, Local, Value},
    JsRuntime, RuntimeOptions,
};
use hyper::body::Bytes;

use crate::{
    analysis::routes::{verify_model_routes, FlatRoutes},
    file::parser::parse_webx_source,
    file::webx::{
        WXBody, WXBodyType, WXModule, WXModulePath, WXRouteHandlerCall, WXTypedIdentifier,
        WXUrlPath, WXUrlPathSegment,
    },
    reporting::{debug::info, error::error_code, route::print_route, warning::warning},
    runner::WXMode,
    timeout_duration,
};

use super::{
    http::responses::{self, ok_html, ok_json},
    stdlib,
};

/// A runtime error.
#[derive(Debug, PartialEq, Clone)]
pub struct WXRuntimeError {
    pub code: i32,
    pub message: String,
}

impl Display for WXRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for WXRuntimeError {}

#[derive(Debug, PartialEq, Clone)]
pub struct WXRTContext {
    pub values: HashMap<String, Global<Value>>,
}

impl WXRTContext {
    pub fn new() -> WXRTContext {
        WXRTContext {
            values: HashMap::new(),
        }
    }

    pub fn bind(&mut self, key: &str, value: Global<Value>) {
        self.values.insert(key.to_string(), value);
    }
}

fn init_context<'a>(
    scope: &'a mut v8::HandleScope,
    ctx: &WXRTContext,
) -> v8::Local<'a, v8::Context> {
    let js_ctx = v8::Context::new(scope);
    let global = js_ctx.global(scope);
    for (key, value) in ctx.values.iter() {
        // Maybe replace key with Symbol instead?
        let key = v8::String::new(scope, key).unwrap();
        let value = Local::new(scope, value);
        global.set(scope, key.into(), value);
    }
    js_ctx
}

fn eval_js_expression(
    expr: String,
    rt: &mut JsRuntime,
    ctx: &WXRTContext,
) -> Result<Global<Value>, WXRuntimeError> {
    {
        let mut scope = rt.handle_scope();
        init_context(&mut scope, ctx);
    }
    let val = rt.execute_script("[webx expression]", expr.into());
    match val {
        Ok(val) => Ok(val),
        Err(err) => Err(WXRuntimeError {
            code: 500,
            message: format!("Expression threw an error:\n{}", err),
        }),
    }
}
impl WXRouteHandlerCall {
    /// Execute the handler in the given context and return the result.
    fn execute(
        &self,
        ctx: &WXRTContext,
        rt: &mut JsRuntime,
        info: &WXRuntimeInfo,
    ) -> Result<Global<Value>, WXRuntimeError> {
        match self.try_execute_native_script(rt, ctx, info) {
            Some(result) => result,
            None => self.execute_user_script(rt),
        }
    }

    fn extract_arguments(
        &self,
        global_args: v8::Global<v8::Value>,
        rt: &mut JsRuntime,
    ) -> Result<Vec<Global<Value>>, WXRuntimeError> {
        let mut js_args = Vec::new();

        {
            // Isolate the HandleScope to this block to avoid lifetime issues
            let scope = &mut rt.handle_scope();
            let local_args = Local::new(scope, global_args);

            let arr_args = match Local::<'_, v8::Array>::try_from(local_args) {
                Ok(args) => args,
                Err(err) => {
                    return Err(WXRuntimeError {
                        code: 500,
                        message: format!(
                            "Handler '{}' expected an array, got: {:?} and failed with error: {:?}",
                            self.name, local_args, err
                        ),
                    })
                }
            };

            let len = arr_args.length() as usize;

            for i in 0..len {
                let arg = arr_args.get_index(scope, i as u32).unwrap();
                let global_arg = Global::new(scope, arg);
                js_args.push(global_arg);
            }
        }

        Ok(js_args)
    }

    fn try_execute_native_script(
        &self,
        rt: &mut JsRuntime,
        ctx: &WXRTContext,
        info: &WXRuntimeInfo,
    ) -> Option<Result<Global<Value>, WXRuntimeError>> {
        let global_args = match eval_js_expression(format!("[{}]", self.args), rt, ctx) {
            Ok(val) => val,
            Err(err) => {
                return Some(Err(WXRuntimeError {
                    code: 500,
                    message: format!("Handler '{}' threw an error:\n{}", self.name, err),
                }))
            }
        };
        let js_args = match self.extract_arguments(global_args, rt) {
            Ok(args) => args,
            Err(err) => return Some(Err(err)),
        };
        stdlib::try_call(&self.name, &js_args, rt, info)
    }

    fn execute_user_script(&self, rt: &mut JsRuntime) -> Result<Global<Value>, WXRuntimeError> {
        let js_call = format!("{}({})", self.name, self.args);
        let call_res = rt.execute_script("[webx handler call]", js_call.into());
        call_res.map_err(|e| WXRuntimeError {
            code: 500,
            message: format!("Handler '{}' threw an error:\n{}", self.name, e),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WXPathResolution {
    None,
    Perfect(HashMap<String, String>),
}

impl WXUrlPath {
    fn get_url_segments(url: &hyper::Uri) -> Vec<&str> {
        url.path()
            .split('/')
            .skip(1)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    }

    pub fn matches(&self, url: &hyper::Uri) -> WXPathResolution {
        let parts = WXUrlPath::get_url_segments(url);
        if self.segments() != parts.len() {
            return WXPathResolution::None;
        }
        let mut bindings = HashMap::new();
        for (pattern, part) in self.0.iter().zip(parts) {
            match pattern {
                WXUrlPathSegment::Literal(literal) if literal != part => {
                    return WXPathResolution::None
                }
                WXUrlPathSegment::Parameter(WXTypedIdentifier { name, .. }) => {
                    bindings.insert(name.clone(), part.to_owned());
                }
                WXUrlPathSegment::Regex(name, pattern) => {
                    if pattern != "*"
                        && !regex::Regex::new(&format!("^(?:{})$", pattern))
                            .is_ok_and(|re| re.is_match(part))
                    {
                        return WXPathResolution::None;
                    }
                    bindings.insert(name.clone(), part.to_owned());
                }
                _ => {}
            }
        }
        WXPathResolution::Perfect(bindings)
    }
}

/// Possible values produced by a handler or route body.
pub enum WXRouteResult {
    Html(String),
    Js(Global<Value>),
}

/// A runtime flat-route.
#[derive(Debug, Clone)]
pub struct WXRTRoute {
    // TODO: Add support for:
    // TODO: - handler functions
    // TODO: - global typescript code
    // TODO: - models ORM and types
    module_path: WXModulePath,
    body: Option<WXBody>,
    pre_handlers: Vec<WXRouteHandlerCall>,
    post_handlers: Vec<WXRouteHandlerCall>,
}

impl WXRTRoute {
    fn execute_body(
        &self,
        _ctx: &mut WXRTContext,
        _rt: &mut JsRuntime,
        _info: &WXRuntimeInfo,
    ) -> Result<WXRouteResult, WXRuntimeError> {
        let Some(body) = &self.body else {
            return Err(WXRuntimeError {
                code: 500,
                message: "Route body is empty".into(),
            });
        };
        match body.body_type {
            WXBodyType::Ts => todo!("TS body type is not supported yet"),
            // TODO: - Resolve bindings, render and execute JSX (dynamic)
            // TODO: - Use JSX runtime to render JSX
            WXBodyType::Tsx => Ok(WXRouteResult::Html(body.body.clone())),
        }
    }

    fn execute_handlers(
        &self,
        handlers: &[WXRouteHandlerCall],
        ctx: &mut WXRTContext,
        rt: &mut JsRuntime,
        info: &WXRuntimeInfo,
    ) -> Option<Result<WXRouteResult, WXRuntimeError>> {
        let mut handlers = handlers.iter();
        for _ in 0..self.pre_handlers.len() - 1 {
            let handler = handlers.next().unwrap();
            let result = match handler.execute(ctx, rt, info) {
                Ok(result) => result,
                Err(err) => return Some(Err(err)),
            };
            if let Some(output) = &handler.output {
                ctx.bind(output, result);
            }
        }
        handlers
            .last()
            .map(|last| last.execute(ctx, rt, info).map(WXRouteResult::Js))
    }

    fn bind_out(ctx: &mut WXRTContext, value: WXRouteResult, scope: &mut v8::HandleScope) {
        match value {
            WXRouteResult::Html(s) => {
                let handle: Local<'_, v8::Value> =
                    v8::String::new(scope, s.as_str()).unwrap().into();
                ctx.bind("out", v8::Global::new(scope, handle))
            }
            WXRouteResult::Js(v) => ctx.bind("out", v),
        }
    }

    fn to_response(
        value: WXRouteResult,
        scope: &mut v8::HandleScope,
        mode: WXMode,
    ) -> hyper::Response<hyper::body::Bytes> {
        match value {
            WXRouteResult::Html(body) => {
                let body = hyper::body::Bytes::from(body);
                let len = body.len();
                ok_html(body, len, mode)
            }
            WXRouteResult::Js(value) => {
                if let Ok(str_val) =
                    Local::<'_, v8::String>::try_from(Local::new(scope, value.clone()))
                {
                    let str = hyper::body::Bytes::from(str_val.to_rust_string_lossy(scope));
                    let len = str.len();
                    ok_html(str, len, mode)
                } else {
                    ok_json(&value, scope, mode)
                }
            }
        }
    }

    /// Execute the route and return a HTTP response.
    ///
    /// ## Note
    /// This function will **not** check if the route is valid.
    ///
    fn execute(
        &self,
        ctx: &mut WXRTContext,
        rt: &mut JsRuntime,
        info: &WXRuntimeInfo,
        mode: WXMode,
    ) -> Result<hyper::Response<hyper::body::Bytes>, WXRuntimeError> {
        // TODO: Refactor this function to combine all logic into a better structure.
        let has_pre_handlers: bool = !self.pre_handlers.is_empty();
        let has_body: bool = self.body.is_some();
        let has_post_handlers: bool = !self.post_handlers.is_empty();
        return match (has_pre_handlers, has_body, has_post_handlers) {
			// All three are present, execute pre-handlers, body, and post-handlers.
            (true, true, true) => {
                self.execute_handlers(&self.pre_handlers, ctx, rt, info);
                let value = self.execute_body(ctx, rt, info)?;
				Self::bind_out(ctx, value, &mut rt.handle_scope());
                Ok(Self::to_response(
                    self.execute_handlers(&self.post_handlers, ctx, rt, info)
                        .unwrap()?,
                    &mut rt.handle_scope(),
                    mode,
                ))
            }
			// Execute pre-handlers and body.
			(true, true, false) => {
                self.execute_handlers(&self.pre_handlers, ctx, rt, info);
                Ok(Self::to_response(
                    self.execute_body(ctx, rt, info)?,
                    &mut rt.handle_scope(),
                    mode,
                ))
			}
			// Execute pre and post-handlers.
			(true, false, true) => {
                self.execute_handlers(&self.pre_handlers, ctx, rt, info);
                Ok(Self::to_response(
                    self.execute_handlers(&self.post_handlers, ctx, rt, info).unwrap()?,
                    &mut rt.handle_scope(),
                    mode,
                ))
			}
			// Execute only pre-handlers.
			(true, false, false) => Ok(Self::to_response(
				self.execute_handlers(&self.pre_handlers, ctx, rt, info).unwrap()?,
				&mut rt.handle_scope(),
				mode,
			)),
			// Execute body and post-handlers.
			(false, true, true) => {
                let value = self.execute_body(ctx, rt, info)?;
				Self::bind_out(ctx, value, &mut rt.handle_scope());
                Ok(Self::to_response(
                    self.execute_handlers(&self.post_handlers, ctx, rt, info)
                        .unwrap()?,
                    &mut rt.handle_scope(),
                    mode,
                ))
            }
			// Execute only body
            (false, true, false) => Ok(Self::to_response(
                self.execute_body(ctx, rt, info)?,
                &mut rt.handle_scope(),
                mode,
            )),
			// Execute only post-handlers
            (false, false, true) => Ok(Self::to_response(
				self.execute_handlers(&self.post_handlers, ctx, rt, info).unwrap()?,
				&mut rt.handle_scope(),
				mode,
			)),
            (false, false, false) => Err(WXRuntimeError {
                code: 500,
                message: format!("Route execution not implemented for: pre_handlers={}, body={}, post_handlers={}", has_pre_handlers, has_body, has_post_handlers),
            }),
        };
    }
}

type WXMethodMapInner = Vec<(WXUrlPath, WXRTRoute)>;
type WXRouteMapInner = HashMap<hyper::Method, WXMethodMapInner>;

/// This is a map of all routes in the project.
/// The key is the route path, and the value is the route.
/// This map requires that **all routes are unique**.
/// This is enforced by the `analyze_module_routes` function.
#[derive(Debug, Clone)]
pub struct WXRouteMap(WXRouteMapInner);

impl WXRouteMap {
    fn new() -> Self {
        WXRouteMap(HashMap::new())
    }

    /// Create a new route map from a list of modules.
    fn from_modules(modules: &[WXModule]) -> Result<Self, WXRuntimeError> {
        let routes: FlatRoutes = verify_model_routes(modules)?;
        let mut route_map: WXRouteMapInner = HashMap::new();
        // Insert all routes into each method map category.
        for (route, path) in routes {
            route_map.entry(route.method.clone()).or_default().push((
                path.clone(),
                WXRTRoute {
                    module_path: route.info.path,
                    body: route.body,
                    pre_handlers: route.pre_handlers,
                    post_handlers: route.post_handlers,
                },
            ));
        }
        Ok(WXRouteMap(route_map))
    }

    /// Get a route from the route map.
    /// This function will return `None` if the route does not exist.
    ///
    /// ## Note
    /// This function will **not** check for duplicate routes.
    /// This is done in the `analyze_module_routes` function.
    fn resolve(
        &self,
        method: &hyper::Method,
        path: &hyper::Uri,
    ) -> Option<(&WXUrlPath, HashMap<String, String>, &WXRTRoute)> {
        // The table is sorted once at compilation, independently of hash iteration.
        for (route_path, route) in self.0.get(method)? {
            if let WXPathResolution::Perfect(bindings) = route_path.matches(path) {
                return Some((route_path, bindings, route));
            }
        }
        None
    }
}

/// Channel message for the runtime.
pub enum WXRuntimeMessage {
    Shutdown,
    Reload,
    ExecuteRoute {
        request: hyper::Request<hyper::body::Incoming>,
        addr: SocketAddr,
        respond_to: tokio::sync::oneshot::Sender<
            Result<hyper::Response<http_body_util::Full<hyper::body::Bytes>>, WXRuntimeError>,
        >,
    },
}
#[derive(Clone)]
pub struct WXRuntimeInfo {
    pub project_root: Box<Path>,
    pub source_root: PathBuf,
}

impl WXRuntimeInfo {
    pub fn new(project_root: &Path) -> Self {
        WXRuntimeInfo {
            project_root: project_root.to_path_buf().into_boxed_path(),
            source_root: project_root.to_path_buf(),
        }
    }
}

/// The WebX runtime.
pub struct WXRuntime {
    mode: WXMode,
    info: WXRuntimeInfo,
    source_modules: Vec<WXModule>,
    last_sources: Option<HashMap<PathBuf, String>>,
    messages: Receiver<WXRuntimeMessage>,
    routes: WXRouteMap,
    /// Module globals persist across requests and HTML-only edits. A changed
    /// global block replaces its module runtime only after successful validation.
    modules: HashMap<WXModulePath, deno_core::JsRuntime>,
}

impl WXRuntime {
    pub fn new(rx: Receiver<WXRuntimeMessage>, mode: WXMode, info: WXRuntimeInfo) -> Self {
        WXRuntime {
            source_modules: Vec::new(),
            last_sources: None,
            routes: WXRouteMap::new(),
            messages: rx,
            mode,
            info,
            modules: HashMap::new(),
        }
    }

    /// Load a list of modules into the runtime.
    ///
    /// ## Note
    /// This function will **not** recompile the route map.
    /// To recompile the route map, either:
    /// - start the runtime with the `run` function.
    /// - trigger a module hot-swap in `dev` mode.
    pub fn load_modules(&mut self, modules: Vec<WXModule>) {
        modules.into_iter().for_each(|m| self.load_module(m));
        // The first development request rechecks the exact on-disk snapshot.
        self.last_sources = None;
    }

    /// Load a single module into the runtime.
    /// This function will **NOT** recompile the route map.
    /// To recompile the route map, either:
    /// - start the runtime with the `run` function.
    /// - trigger a module hot-swap in `dev` mode.
    /// - call the `recompile` function.
    ///
    /// ## Note
    /// Only call this function once per module.
    /// This should **NOT** be called when hot-swapping modules.
    pub fn load_module(&mut self, module: WXModule) {
        let rt = self
            .new_module_js_runtime(&module)
            .expect("Failed to initialize module");
        self.modules.insert(module.path.clone(), rt);
        self.source_modules.push(module);
    }

    /// Build a candidate runtime without disturbing the currently serving module.
    fn new_module_js_runtime(&self, module: &WXModule) -> Result<JsRuntime, WXRuntimeError> {
        let mut rt = JsRuntime::new(RuntimeOptions {
            module_loader: Some(Rc::new(deno_core::FsModuleLoader)),
            ..Default::default()
        });
        rt.execute_script(
            "[webx stdlib]",
            deno_core::FastString::Static(stdlib::JAVASCRIPT),
        )
        .map_err(|err| WXRuntimeError {
            code: 500,
            message: format!("Failed to execute stdlib: {}", err),
        })?;
        rt.execute_script("[global scope]", module.scope.global_ts.to_owned().into())
            .map_err(|err| WXRuntimeError {
                code: 500,
                message: format!(
                    "Failed to initialize {}: {}",
                    module.path.module_name(),
                    err
                ),
            })?;
        Ok(rt)
    }

    /// Reconcile the whole source tree atomically. Checking here as well as on
    /// watcher events guarantees that the first request after a save sees it.
    fn refresh_sources(&mut self) -> Result<(), WXRuntimeError> {
        fn read_sources(root: &Path, files: &mut HashMap<PathBuf, String>) -> std::io::Result<()> {
            for entry in std::fs::read_dir(root)? {
                let entry = entry?;
                let path = entry.path();
                if entry.file_type()?.is_dir() {
                    read_sources(&path, files)?;
                } else if matches!(
                    path.extension().and_then(|s| s.to_str()),
                    Some("wx" | "webx")
                ) {
                    files.insert(path.canonicalize()?, std::fs::read_to_string(path)?);
                }
            }
            Ok(())
        }
        let mut sources = HashMap::new();
        read_sources(&self.info.source_root, &mut sources)?;
        if self.last_sources.as_ref() == Some(&sources) {
            return Ok(());
        }
        // Remember invalid snapshots too: report once, then retry when bytes change.
        self.last_sources = Some(sources.clone());
        let mut paths: Vec<_> = sources.keys().collect();
        paths.sort();
        let modules: Vec<_> = paths
            .into_iter()
            .map(|path| {
                parse_webx_source(path, &sources[path]).map_err(|err| WXRuntimeError {
                    code: 500,
                    message: format!("Reload rejected: {:?}", err),
                })
            })
            .collect::<Result<_, _>>()?;
        let routes = WXRouteMap::from_modules(&modules)?;
        let mut replacements = HashMap::new();
        for module in &modules {
            let previous = self
                .source_modules
                .iter()
                .find(|old| old.path == module.path);
            if previous.is_none_or(|old| old.scope.global_ts != module.scope.global_ts) {
                replacements.insert(module.path.clone(), self.new_module_js_runtime(module)?);
            }
        }
        // Commit only after parsing, route validation, and JS initialization succeed.
        self.modules
            .retain(|path, _| modules.iter().any(|module| &module.path == path));
        self.modules.extend(replacements);
        self.source_modules = modules;
        self.routes = routes;
        info(self.mode, "Reloaded source snapshot");
        Ok(())
    }

    /// Tries to recompile all loaded modules at once and replace the runtime route map.
    ///
    /// ## Note
    /// This way we can get cross-module route analysis, which is required for detecting
    /// duplicate routes and other global errors.
    ///
    /// ## Error
    /// This function will **throw and error** if the route map cannot be compiled
    /// from the current source modules, and will **not** replace the current route map.
    /// However, the program will **continue to run with the old route map**.
    fn recompile(&mut self) {
        match WXRouteMap::from_modules(&self.source_modules) {
            Ok(routes) => self.routes = routes,
            Err(err) => error_code(err.message, err.code, self.mode.date_specifier()),
        }
        if self.mode.is_dev() && self.mode.debug_level().is_high() {
            // Print the route map in dev mode.
            info(self.mode, "Route map:");
            let routes: Vec<(&hyper::Method, &WXUrlPath)> = self
                .routes
                .0
                .iter()
                .flat_map(|(method, method_map)| {
                    method_map
                        .iter()
                        .map(|(path, _)| (method, path))
                        .collect::<Vec<_>>()
                })
                .collect();
            for (method, path) in routes {
                println!(" - {}", print_route(method, path));
            }
        }
    }

    /// Main runtime loop.
    /// This function will run forever in a dedicated thread
    /// and will handle all incoming messages from the channel
    /// until the program is terminated.
    ///
    /// ## Example messages:
    /// - Execute a route within the runtime and return the result.
    ///     - TODO: Such tasks will be executed in a new separate tokio task/thread.
    /// - Hot-swap module in dev mode.
    ///
    /// ## Note
    /// This is **required** as `deno_core::JsRuntime` is **not** thread-safe
    /// and cannot be shared between threads.
    pub fn run(&mut self, running: Arc<AtomicBool>) {
        self.recompile();
        loop {
            if !running.load(Ordering::SeqCst) {
                // println!("Shutting down runtime...");
                break; // Exit the loop and stop the runtime.
            }
            if let Ok(msg) = self.messages.recv_timeout(timeout_duration(self.mode)) {
                match msg {
                    WXRuntimeMessage::Shutdown => break,
                    WXRuntimeMessage::Reload => {
                        if self.mode.is_dev() {
                            if let Err(err) = self.refresh_sources() {
                                error_code(err.message, err.code, self.mode.date_specifier());
                            }
                        }
                    }
                    WXRuntimeMessage::ExecuteRoute {
                        request,
                        addr,
                        respond_to,
                    } => {
                        // A disconnected client must not stop the runtime.
                        let _ = respond_to.send(self.execute_route(request, addr));
                    }
                }
            }
        }
    }

    fn execute_route(
        &mut self,
        req: hyper::Request<hyper::body::Incoming>,
        addr: SocketAddr,
    ) -> Result<hyper::Response<http_body_util::Full<Bytes>>, WXRuntimeError> {
        if self.mode.is_dev() {
            if let Err(err) = self.refresh_sources() {
                error_code(err.message, err.code, self.mode.date_specifier());
            }
        }
        if let Some((_path, bindings, route)) = self.routes.resolve(req.method(), req.uri()) {
            let module_runtime = self.modules.get_mut(&route.module_path).unwrap();
            // V8 handles must be allocated in the runtime that will consume them.
            let mut ctx = WXRTContext::new();
            {
                let scope = &mut module_runtime.handle_scope();
                for (name, value) in bindings {
                    let value: Local<Value> = v8::String::new(scope, &value).unwrap().into();
                    ctx.bind(&name, Global::new(scope, value));
                }
            }
            let route_result = route.execute(&mut ctx, module_runtime, &self.info, self.mode);
            let response = match route_result {
                Ok(response) => response,
                Err(err) => {
                    error_code(
                        err.message.to_string(),
                        err.code,
                        self.mode.date_specifier(),
                    );
                    responses::internal_server_error_default_webx(self.mode, err.message)
                }
            };
            if self.mode.debug_level().is_max() {
                info(
                    self.mode,
                    &format!("Response to: {}\n{}", addr, responses::serialize(&response)),
                );
            } else if self.mode.debug_level().is_high() {
                info(self.mode, &format!("Response to: {}", addr));
            }

            Ok(response.map(http_body_util::Full::from))
        } else {
            warning(self.mode, format!("No route match: {}", req.uri().path()));
            let response =
                responses::not_found_default_webx(self.mode, req.method(), req.uri().to_string());
            info(
                self.mode,
                &format!("{} response to: {}", response.status(), addr),
            );
            Ok(response.map(http_body_util::Full::from))
        }
    }
}
