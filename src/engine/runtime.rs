use std::{
    collections::HashMap,
    fmt::Display,
    net::SocketAddr,
    path::Path,
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
    file::webx::{
        WXBody, WXBodyType, WXModule, WXModulePath, WXRouteHandlerCall, WXTypedIdentifier,
        WXUrlPath, WXUrlPathSegment,
    },
    reporting::{
        debug::info,
        error::{error_code, exit_error, ERROR_EXEC_ROUTE},
        route::print_route,
        warning::warning,
    },
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
    Perfect(WXRTContext),
    Partial(WXRTContext),
}

impl WXUrlPath {
    fn get_url_segments(url: &hyper::Uri) -> Vec<&str> {
        url.path()
            .split('/')
            .skip(1)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    }

    pub fn matches<'a, 'b: 'a>(&self, url: &hyper::Uri) -> WXPathResolution {
        let url = WXUrlPath::get_url_segments(url);
        let url_count = url.len();
        // dbg!(url.clone().collect::<Vec<_>>(), url_count, self.segments());
        let mut bindings = WXRTContext::new();
        let mut isolate = v8::Isolate::new(Default::default());
        let mut scope = v8::HandleScope::new(&mut isolate);

        let match_segment = |(pattern, part): (&WXUrlPathSegment, &&str)| -> bool {
            match pattern {
                WXUrlPathSegment::Literal(literal) => literal.as_str() == *part,
                WXUrlPathSegment::Parameter(WXTypedIdentifier { name, type_: _ }) => {
                    // TODO: Check type.
                    let js_value: Local<'_, Value> =
                        v8::String::new(&mut scope, part).unwrap().into();
                    let js_value: Global<v8::Value> = Global::new(&mut scope, js_value);
                    bindings.bind(name, js_value);
                    true
                }
                WXUrlPathSegment::Regex(regex_name, regex) => {
                    let re = regex::Regex::new(regex).unwrap();
                    if re.is_match(part) {
                        let js_value: Local<'_, Value> =
                            v8::String::new(&mut scope, part).unwrap().into();
                        let js_value: Global<v8::Value> = Global::new(&mut scope, js_value);
                        bindings.bind(regex_name, js_value);
                        true
                    } else {
                        false
                    }
                }
            }
        };

        if self.segments() == url_count {
            if self.0.iter().zip(&url).all(match_segment) {
                return WXPathResolution::Perfect(bindings);
            }
        } else if self.segments() > url_count
            && self
                .0
                .iter()
                .zip(url.iter().chain(std::iter::repeat(&"")))
                .all(match_segment)
            && url_count == self.segments() - 1
        {
            return WXPathResolution::Partial(bindings);
        }

        WXPathResolution::None
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

type WXMethodMapInner = HashMap<WXUrlPath, WXRTRoute>;
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
        for ((route, path), _) in routes {
            route_map.entry(route.method.clone()).or_default().insert(
                path.clone(),
                WXRTRoute {
                    module_path: route.info.path,
                    body: route.body,
                    pre_handlers: route.pre_handlers,
                    post_handlers: route.post_handlers,
                },
            );
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
    ) -> Option<(&WXUrlPath, WXRTContext, &WXRTRoute)> {
        let routes = self.0.get(method)?;
        // Sort all routes by path length in descending order.
        // This is required to ensure that the most specific routes are matched first.
        let mut routes: Vec<(&WXUrlPath, &WXRTRoute)> = routes.iter().collect();
        routes.sort_by(|(a, _), (b, _)| b.segments().cmp(&a.segments()));
        // Go through all routes and try to match the path.
        let mut best_match = None;
        for (route_path, route) in routes {
            match route_path.matches(path) {
                WXPathResolution::None => continue,
                WXPathResolution::Perfect(bindings) => {
                    best_match = Some((route_path, bindings, route));
                    break;
                }
                WXPathResolution::Partial(bindings) => {
                    best_match = Some((route_path, bindings, route));
                }
            }
        }
        best_match
    }
}

/// Channel message for the runtime.
pub enum WXRuntimeMessage {
    New(WXModule),
    Swap(WXModule),
    Remove(WXModulePath),
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
}

impl WXRuntimeInfo {
    pub fn new(project_root: &Path) -> Self {
        WXRuntimeInfo {
            project_root: project_root.to_path_buf().into_boxed_path(),
        }
    }
}

/// The WebX runtime.
pub struct WXRuntime {
    mode: WXMode,
    info: WXRuntimeInfo,
    source_modules: Vec<WXModule>,
    messages: Receiver<WXRuntimeMessage>,
    routes: WXRouteMap,
    /// A WebX TypeScript runtime.
    ///
    /// ## Hot-swapping
    /// These are persistent between hot-swapping modules in dev mode.
    /// They are only created or destroyed when modules are added or removed.
    /// This allows us to keep the state of the application between hot-swaps.
    ///
    /// ## Persistent state
    /// Each JS runtime maintains a JavaScript execution context,
    /// which means that it keeps track of its own persistent state, variables,
    /// functions, and other constructs will persist between script executions
    /// as long as they are run in the same runtime instance.
    modules: HashMap<WXModulePath, deno_core::JsRuntime>,
}

impl WXRuntime {
    pub fn new(rx: Receiver<WXRuntimeMessage>, mode: WXMode, info: WXRuntimeInfo) -> Self {
        WXRuntime {
            source_modules: Vec::new(),
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
        let rt = self.new_module_js_runtime(&module);
        self.modules.insert(module.path.clone(), rt);
        self.source_modules.push(module);
    }

    fn remove_module(&mut self, path: &WXModulePath) {
        self.modules.remove(path);
        self.source_modules.retain(|m| m.path != *path);
    }

    /// Initialize the JavaScript runtime with the stdlib.
    fn new_js_runtime(&mut self) -> JsRuntime {
        let mut rt = JsRuntime::new(RuntimeOptions {
            module_loader: Some(Rc::new(deno_core::FsModuleLoader)),
            // extensions: vec![stdlib::init()],
            ..Default::default()
        });
        // Load WebX Standard Library
        if let Err(err) = rt.execute_script(
            "[webx stdlib]",
            deno_core::FastString::Static(stdlib::JAVASCRIPT),
        ) {
            exit_error(
                format!("Failed to execute stdlib:\n{}", err),
                500,
                self.mode.date_specifier(),
            );
        }
        info(self.mode, "Loaded WebX Standard Library");
        rt
    }

    /// Initialize the module and execute the global scope
    fn new_module_js_runtime(&mut self, module: &WXModule) -> JsRuntime {
        let mut rt = self.new_js_runtime();
        info(
            self.mode,
            &format!("Initializing module '{}'...", module.path.relative()),
        );
        if let Err(err) =
            rt.execute_script("[global scope]", module.scope.global_ts.to_owned().into())
        {
            error_code(
                format!(
                    "Failed to execute global scope for module '{}':\n{}",
                    module.path.relative(),
                    err
                ),
                500,
                self.mode.date_specifier(),
            );
        }
        info(self.mode, "Successfully initialized module!");
        rt
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
        self.routes = match WXRouteMap::from_modules(&self.source_modules) {
            Ok(routes) => routes,
            Err(err) => {
                error_code(err.message, err.code, self.mode.date_specifier());
                return;
            }
        };
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
                    WXRuntimeMessage::New(module) => {
                        info(
                            self.mode,
                            &format!("New module: '{}'", module.path.relative()),
                        );
                        self.load_module(module);
                        self.recompile();
                    }
                    WXRuntimeMessage::Swap(module) => {
                        info(
                            self.mode,
                            &format!("Reloaded module: '{}'", module.path.relative()),
                        );
                        // Module JS runtime is persistent between hot-swaps.
                        self.remove_module(&module.path);
                        self.load_module(module);
                        self.recompile();
                    }
                    WXRuntimeMessage::Remove(path) => {
                        info(self.mode, &format!("Removed module: '{}'", path.relative()));
                        self.remove_module(&path);
                        self.recompile();
                    }
                    WXRuntimeMessage::ExecuteRoute {
                        request,
                        addr,
                        respond_to,
                    } => respond_to
                        .send(self.execute_route(request, addr))
                        .expect("Sending ExecuteRoute response"),
                }
            }
        }
    }

    fn execute_route(
        &mut self,
        req: hyper::Request<hyper::body::Incoming>,
        addr: SocketAddr,
    ) -> Result<hyper::Response<http_body_util::Full<Bytes>>, WXRuntimeError> {
        if let Some((_path, mut ctx, route)) = self.routes.resolve(req.method(), req.uri()) {
            info(self.mode, "Loaded modules:");
            for (m, _) in self.modules.iter() {
                println!(" - {}", m.relative());
            }

            let Some(module_runtime) = self.modules.get_mut(&route.module_path) else {
                return Err(WXRuntimeError {
                    code: ERROR_EXEC_ROUTE,
                    message: "Failed to get module from route".into(),
                });
            };
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
