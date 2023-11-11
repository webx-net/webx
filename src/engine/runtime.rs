use std::{
    borrow::Borrow,
    collections::HashMap,
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    sync::mpsc::Receiver,
};

use deno_core::{
    v8::{self, GetPropertyNamesArgs},
    JsRuntime,
};
use http::{Method, Response, Uri};

use crate::{
    analysis::routes::verify_model_routes,
    file::webx::{
        WXBody, WXBodyType, WXLiteralValue, WXModule, WXModulePath, WXRoute, WXRouteHandler,
        WXTypedIdentifier, WXUrlPath, WXUrlPathSegment,
    },
    reporting::{debug::info, error::error_code, warning::warning},
    runner::WXMode,
};

use super::{
    http::{
        parse_request_from_string, read_all_from_stream,
        responses::{self, ok_html},
        serialize_response,
    },
    stdlib,
};

/// A runtime error.
pub struct WXRuntimeError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, PartialEq, Clone)]
pub struct WXRTContext {
    pub values: HashMap<String, WXRTValue>,
}

impl WXRTContext {
    fn new() -> Self {
        WXRTContext {
            values: HashMap::new(),
        }
    }

    fn bind(&mut self, ident: &str, value: WXRTValue) {
        self.values.insert(ident.into(), value);
    }

    fn resolve(&self, ident: &str) -> Option<WXRTValue> {
        self.values.get(ident).cloned()
    }
}

/// Runtime values in WebX.
#[derive(Debug, PartialEq, Clone)]
pub enum WXRTValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
    Array(Vec<WXRTValue>),
    Object(Vec<(String, WXRTValue)>),
}

impl WXRTValue {
    /// Convert the runtime value into a string representing a JavaScript value.
    pub fn to_js(&self) -> String {
        match self {
            WXRTValue::String(s) => format!("\"{}\"", s),
            WXRTValue::Number(f) => f.to_string(),
            WXRTValue::Boolean(b) => b.to_string(),
            WXRTValue::Null => "null".into(),
            WXRTValue::Array(arr) => {
                let mut values = Vec::new();
                for value in arr.iter() {
                    values.push(value.to_js());
                }
                format!("[{}]", values.join(", "))
            }
            WXRTValue::Object(obj) => {
                let mut values = Vec::new();
                for (key, value) in obj.iter() {
                    values.push(format!("{}: {}", key, value.to_js()));
                }
                format!("{{{}}}", values.join(", "))
            }
        }
    }

    /// Convert the runtime value into a raw value string.
    /// This function will **not** wrap strings in quotes.
    /// This function is used for sanitizing values in JSX render functions to be sent to the client.
    /// This function will **not** escape any characters.
    pub fn to_raw(&self) -> String {
        match self {
            WXRTValue::String(s) => s.clone(),
            WXRTValue::Number(f) => f.to_string(),
            WXRTValue::Boolean(b) => b.to_string(),
            WXRTValue::Null => "null".into(),
            WXRTValue::Array(arr) => {
                let mut values = Vec::new();
                for value in arr.iter() {
                    values.push(value.to_raw());
                }
                format!("[{}]", values.join(", "))
            }
            WXRTValue::Object(obj) => {
                let mut values = Vec::new();
                for (key, value) in obj.iter() {
                    values.push(format!("{}: {}", key, value.to_raw()));
                }
                format!("{{{}}}", values.join(", "))
            }
        }
    }

    pub fn from_js_value(val: &v8::Value) -> Result<Self, String> {
        let mut isolate = v8::Isolate::new(Default::default());
        let mut handle_scope = v8::HandleScope::new(&mut isolate);
        let context = v8::Context::new(&mut handle_scope);
        let scope = &mut v8::ContextScope::new(&mut handle_scope, context);
        let val: &crate::engine::runtime::v8::Value = val;
        if val.is_undefined() {
            return Ok(WXRTValue::Null);
        }
        if val.is_null() {
            return Ok(WXRTValue::Null);
        }
        if val.is_string() {
            return Ok(WXRTValue::String(val.to_rust_string_lossy(scope)));
        }
        if val.is_number() {
            return Ok(WXRTValue::Number(val.number_value(scope).unwrap()));
        }
        if val.is_boolean() {
            return Ok(WXRTValue::Boolean(val.boolean_value(scope)));
        }
        if val.is_array() {
            let mut values = Vec::new();
            let arr_obj = val.to_object(scope).unwrap();
            let len_str = v8::String::new(scope, "length").unwrap();
            let len = arr_obj.get(scope, len_str.into()).unwrap();
            let len = len.number_value(scope).unwrap() as usize;
            for i in 0..len {
                let val = arr_obj.get_index(scope, i as u32).unwrap();
                let val = WXRTValue::from_js_value(&val).unwrap();
                values.push(val);
            }
            return Ok(WXRTValue::Array(values));
        }
        if val.is_object() {
            let mut fields = Vec::new();
            let obj = val.to_object(scope).unwrap();
            let keys = obj
                .get_own_property_names(scope, GetPropertyNamesArgs::default())
                .unwrap();
            let len = keys.length() as usize;
            for i in 0..len {
                let key = keys.get_index(scope, i as u32).unwrap();
                let key = key.to_string(scope).unwrap();
                let key = key.to_rust_string_lossy(scope);
                let key_str = v8::String::new(scope, &key).unwrap();
                let val = obj.get(scope, key_str.into()).unwrap();
                let val = WXRTValue::from_js_value(&val).unwrap();
                fields.push((key, val));
            }
            return Ok(WXRTValue::Object(fields));
        }
        Err("Unsupported value type".into())
    }
}

fn eval_literal(literal: &WXLiteralValue, ctx: &WXRTContext) -> Result<WXRTValue, WXRuntimeError> {
    match literal {
        WXLiteralValue::String(s) => Ok(WXRTValue::String(s.clone())),
        WXLiteralValue::Number(i, d) => Ok(WXRTValue::Number(
            format!("{}.{}", i, d).parse::<f64>().unwrap(),
        )),
        WXLiteralValue::Boolean(b) => Ok(WXRTValue::Boolean(*b)),
        WXLiteralValue::Null => Ok(WXRTValue::Null),
        WXLiteralValue::Array(arr) => {
            let mut values = Vec::new();
            for value in arr.iter() {
                values.push(eval_literal(value, ctx)?);
            }
            Ok(WXRTValue::Array(values))
        }
        WXLiteralValue::Object(obj) => {
            let mut values = Vec::new();
            for (key, value) in obj.iter() {
                values.push((key.clone(), eval_literal(value, ctx)?));
            }
            Ok(WXRTValue::Object(values))
        }
        WXLiteralValue::Identifier(ident) => {
            if let Some(value) = ctx.resolve(ident) {
                Ok(value)
            } else {
                Err(WXRuntimeError {
                    code: 500,
                    message: format!("Identifier '{}' not found in context", ident),
                })
            }
        }
    }
}

/// A runtime handler call.
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct WXRTHandlerCall {
    /// The handler name.
    pub name: String,
    /// The handler arguments.
    pub args: Vec<WXLiteralValue>,
    /// The output variable name.
    pub output: Option<String>,
}

impl WXRTHandlerCall {
    fn from_handler(handler: &WXRouteHandler) -> Self {
        WXRTHandlerCall {
            name: handler.name.clone(),
            args: handler.args.clone(),
            output: handler.output.clone(),
        }
    }

    /// Execute the handler in the given context and return the result.
    fn execute(
        &self,
        ctx: &WXRTContext,
        rt: &mut JsRuntime,
        info: &WXRuntimeInfo,
    ) -> Result<WXRTValue, WXRuntimeError> {
        let args = self
            .args
            .iter()
            .map(|arg| eval_literal(arg, ctx))
            .collect::<Result<Vec<_>, _>>()?;
        // Try to call a native handler.
        if let Some(native_res) = stdlib::try_call(&self.name, &args, info) {
            return native_res;
        }
        // User-defined handler
        let js_args = args.iter().map(WXRTValue::to_js).collect::<Vec<_>>();
        let js_call = format!("{}({})", self.name, js_args.join(", "));
        match rt.execute_script("[webx handler code]", js_call.into()) {
            Ok(val) => {
                let val: &v8::Value = val.borrow();
                if val.is_null_or_undefined() {
                    return Ok(WXRTValue::Null);
                }
                WXRTValue::from_js_value(val).map_err(|e| WXRuntimeError {
                    code: 500,
                    message: format!("Handler '{}' returned an invalid value:\n{}", self.name, e),
                })
            }
            Err(e) => Err(WXRuntimeError {
                code: 500,
                message: format!("Handler '{}' threw an error:\n{}", self.name, e),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WXPathResolution {
    None,
    Perfect(WXRTContext),
    Partial(WXRTContext),
}

impl WXUrlPath {
    fn get_url_segments(url: &Uri) -> Vec<&str> {
        url.path()
            .split('/')
            .skip(1)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    }

    pub fn matches(&self, url: &Uri) -> WXPathResolution {
        let url = WXUrlPath::get_url_segments(url);
        let url_count = url.len();
        // dbg!(url.clone().collect::<Vec<_>>(), url_count, self.segments());
        let mut bindings = WXRTContext::new();

        let match_segment = |(pattern, part): (&WXUrlPathSegment, &&str)| -> bool {
            match pattern {
                WXUrlPathSegment::Literal(literal) => literal.as_str() == *part,
                WXUrlPathSegment::Parameter(WXTypedIdentifier { name, type_: _ }) => {
                    // TODO: Check type.
                    bindings.bind(name, WXRTValue::String(part.to_string()));
                    true
                }
                WXUrlPathSegment::Regex(regex_name, regex) => {
                    let re = regex::Regex::new(regex).unwrap();
                    if re.is_match(part) {
                        bindings.bind(regex_name, WXRTValue::String(part.to_string()));
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
        } else if self.segments() > url_count && self
                .0
                .iter()
                .zip(url.iter().chain(std::iter::repeat(&"")))
                .all(match_segment) && url_count == self.segments() - 1 {
            return WXPathResolution::Partial(bindings);
        }

        WXPathResolution::None
    }
}
/// A runtime flat-route.
#[derive(Debug)]
pub struct WXRTRoute {
    // TODO: Add support for:
    // TODO: - handler functions
    // TODO: - global typescript code
    // TODO: - models ORM and types
    module_path: WXModulePath,
    body: Option<WXBody>,
    pre_handlers: Vec<WXRTHandlerCall>,
    post_handlers: Vec<WXRTHandlerCall>,
}

impl WXRTRoute {
    fn execute_body(&self, _ctx: &mut WXRTContext) -> Result<WXRTValue, WXRuntimeError> {
        assert!(self.body.is_some());
        let body = self.body.as_ref().unwrap();
        match body.body_type {
            WXBodyType::TS => todo!("TS body type is not supported yet"),
            // TODO: Resolve bindings, render and execute JSX (dynamic)
            WXBodyType::TSX => Ok(WXRTValue::String(body.body.clone())),
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
    ) -> Result<Response<String>, WXRuntimeError> {
        // TODO: Refactor this function to combine all logic into a better structure.
        if self.pre_handlers.is_empty() && self.body.is_none() && self.post_handlers.is_empty() {
            // No handlers or body are present, return an empty response.
            Err(WXRuntimeError {
                code: 500,
                message: "Route is empty".into(),
            })
        } else if self.pre_handlers.is_empty()
            && self.body.is_some()
            && self.post_handlers.is_empty()
        {
            // Only a body is present, execute it and return the result.
            Ok(ok_html(self.execute_body(ctx)?.to_raw()))
        } else if self.body.is_none() {
            // Only handlers are present, execute them sequentially.
            // Merge all pre and post handlers into a single handler vector.
            let mut handlers = self.pre_handlers.clone();
            handlers.extend(self.post_handlers.clone());
            // Execute all (but last() handlers sequentially.
            for handler in handlers.iter().take(handlers.len() - 1) {
                let result = handler.execute(ctx, rt, info)?;
                // Bind the result to the output variable.
                if let Some(output) = &handler.output {
                    ctx.bind(output, result);
                }
            }
            // Execute the last handler and return the result as the response.
            let handler = handlers.last().unwrap();
            Ok(ok_html(handler.execute(ctx, rt, info)?.to_raw()))
        } else {
            // Both handlers and a body are present.
            // Execute pre-handlers sequentially.
            for handler in self.pre_handlers.iter() {
                let result = handler.execute(ctx, rt, info)?;
                if let Some(output) = &handler.output {
                    ctx.bind(output, result);
                }
            }
            let body = self.execute_body(ctx)?;
            if self.post_handlers.is_empty() {
                // No post-handlers are present, return the body result.
                return Ok(ok_html(body.to_raw()));
            }
            // Execute post-handlers sequentially.
            for handler in self.post_handlers.iter().take(self.post_handlers.len() - 1) {
                let result = handler.execute(ctx, rt, info)?;
                if let Some(output) = &handler.output {
                    ctx.bind(output, result);
                }
            }
            Ok(ok_html(
                self.post_handlers
                    .last()
                    .unwrap()
                    .execute(ctx, rt, info)?
                    .to_raw(),
            ))
        }
    }
}

type WXMethodMapInner = HashMap<WXUrlPath, WXRTRoute>;
type WXRouteMapInner = HashMap<Method, WXMethodMapInner>;

/// This is a map of all routes in the project.
/// The key is the route path, and the value is the route.
/// This map requires that **all routes are unique**.
/// This is enforced by the `analyse_module_routes` function.
#[derive(Debug)]
pub struct WXRouteMap(WXRouteMapInner);

impl WXRouteMap {
    fn new() -> Self {
        WXRouteMap(HashMap::new())
    }

    /// Create a new route map from a list of modules.
    fn from_modules(modules: &Vec<WXModule>) -> Result<Self, WXRuntimeError> {
        let routes = verify_model_routes(modules);
        if let Err((message, code)) = routes {
            return Err(WXRuntimeError { code, message });
        }
        let mut route_map: WXRouteMapInner = HashMap::new();
        // Insert all routes into each method map category.
        for ((route, path), _) in routes.unwrap().iter() {
            let method_map = route_map
                .entry(route.method.clone())
                .or_default();
            method_map.insert(
                path.clone(),
                Self::compile_route(route, route.info.path.clone())?,
            );
        }
        Ok(WXRouteMap(route_map))
    }

    /// Compile a parsed route into a runtime route.
    fn compile_route(
        route: &WXRoute,
        module_path: WXModulePath,
    ) -> Result<WXRTRoute, WXRuntimeError> {
        let body = route.body.clone();
        Ok(WXRTRoute {
            module_path,
            body,
            pre_handlers: route
                .pre_handlers
                .iter()
                .map(WXRTHandlerCall::from_handler)
                .collect(),
            post_handlers: route
                .post_handlers
                .iter()
                .map(WXRTHandlerCall::from_handler)
                .collect(),
        })
    }

    /// Get a route from the route map.
    /// This function will return `None` if the route does not exist.
    ///
    /// ## Note
    /// This function will **not** check for duplicate routes.
    /// This is done in the `analyse_module_routes` function.
    fn resolve(
        &self,
        method: &Method,
        path: &Uri,
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
    Swap(WXModulePath, WXModule),
    Remove(WXModulePath),
}

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
    /// All WebX TypeScript runtimes.
    ///
    /// ## Hotswapping
    /// These are persistent between hotswapping modules in dev mode.
    /// They are only created or destroyed when modules are added or removed.
    /// This allows us to keep the state of the application between hotswaps.
    ///
    /// ## Persistent state
    /// Each JS runtime maintains a JavaScript execution context,
    /// which means that it keeps track of its own persistent state, variables,
    /// functions, and other constructs will persist between script executions
    /// as long as they are run in the same runtime instance.
    runtimes: HashMap<WXModulePath, JsRuntime>,
}

impl WXRuntime {
    pub fn new(rx: Receiver<WXRuntimeMessage>, mode: WXMode, info: WXRuntimeInfo) -> Self {
        WXRuntime {
            source_modules: Vec::new(),
            routes: WXRouteMap::new(),
            messages: rx,
            mode,
            info,
            runtimes: HashMap::new(),
        }
    }

    /// Load a list of modules into the runtime.
    ///
    /// ## Note
    /// This function will **not** recompile the route map.
    /// To recompile the route map, either:
    /// - start the runtime with the `run` function.
    /// - trigger a module hotswap in `dev` mode.
    pub fn load_modules(&mut self, modules: Vec<WXModule>) {
        modules.into_iter().for_each(|m| self.load_module(m));
    }

    /// Load a single module into the runtime.
    /// This function will **NOT** recompile the route map.
    /// To recompile the route map, either:
    /// - start the runtime with the `run` function.
    /// - trigger a module hotswap in `dev` mode.
    /// - call the `recompile` function.
    ///
    /// ## Note
    /// Only call this function once per module.
    /// This should **NOT** be called when hotswapping modules.
    pub fn load_module(&mut self, module: WXModule) {
        self.runtimes
            .insert(module.path.clone(), JsRuntime::new(Default::default()));
        self.initialize_module_runtime(&module);
        self.source_modules.push(module);
    }

    fn remove_module(&mut self, module_path: &WXModulePath) {
        self.runtimes.remove(module_path);
        self.source_modules.retain(|m| m.path != *module_path);
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
            Err(err) => error_code(err.message, err.code),
        }
        if self.mode.is_dev() && self.mode.debug_level().is_high() {
            // Print the route map in dev mode.
            info(self.mode, "Route map:");
            let routes: Vec<(&Method, &WXUrlPath)> = self
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
                println!(" - {} {}", method, path);
            }
        }
    }

    /// Execute the global scope in the runtime for a specific module
    fn initialize_module_runtime(&mut self, module: &WXModule) {
        if let Some(rt) = self.runtimes.get_mut(&module.path) {
            let ts = module.scope.global_ts.clone();
            let result = rt.execute_script("[webx global scope]", ts.into());
            if let Err(e) = result {
                error_code(
                    format!(
                        "Failed to execute global scope for module '{}':\n{}",
                        module.path.module_name(),
                        e
                    ),
                    500,
                );
            }
            info(
                self.mode,
                &format!(
                    "Initialized runtime for module '{}'",
                    module.path.module_name()
                ),
            );
        } else {
            dbg!(&self.runtimes.keys());
            error_code(
                format!(
                    "Module runtime not found for module '{}'",
                    module.path.module_name()
                ),
                500,
            );
        }
    }

    /// Main runtime loop.
    /// This function will run forever in a dedicated thread
    /// and will handle all incoming requests and responses
    /// until the program is terminated.
    pub fn run(&mut self) {
        self.recompile(); // Ensure that we have a valid route map.
        info(self.mode, "WebX server is running!");
        let ports = if self.mode.is_dev() {
            vec![8080]
        } else {
            vec![80, 443]
        };
        let addrs = ports
            .iter()
            .map(|port| SocketAddr::from(([127, 0, 0, 1], *port)))
            .collect::<Vec<_>>();
        let listener = TcpListener::bind(&addrs[..]).unwrap();
        // Don't block if in dev mode, wait and read hotswap messages.
        listener.set_nonblocking(self.mode.is_dev()).unwrap();
        loop {
            self.listen_for_requests(&listener);
            // In dev mode, we don't want the TCP listener to block the thread.
            // Instead, we want to shortly sleep, then check for new messages
            // from the channel to enable module hotswapping.
            if self.mode.is_dev() {
                self.sync_channel_messages();
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    /// Look for module updates from the given channel.
    /// This function is **non-blocking**.
    /// All queued updates are applied immediately.
    fn sync_channel_messages(&mut self) {
        while let Ok(msg) = self.messages.try_recv() {
            match msg {
                WXRuntimeMessage::New(module) => {
                    info(
                        self.mode,
                        &format!("New module: {}", module.path.module_name()),
                    );
                    self.load_module(module);
                    self.recompile();
                }
                WXRuntimeMessage::Swap(path, module) => {
                    info(
                        self.mode,
                        &format!("Reloaded module: {}", module.path.module_name()),
                    );
                    // Module JS runtime is persistent between hotswaps.
                    self.source_modules.retain(|m| m.path != path);
                    self.source_modules.push(module);
                    self.recompile();
                }
                WXRuntimeMessage::Remove(path) => {
                    info(
                        self.mode,
                        &format!("Removed module: {}", path.module_name()),
                    );
                    self.remove_module(&path);
                    self.recompile();
                }
            }
        }
    }

    /// Listen for incoming requests.
    ///
    /// ## Blocking
    /// This function is **non-blocking** and will return immediately depending on the
    /// value of `listener.set_nonblocking` in the `run` function.
    fn listen_for_requests(&mut self, listener: &TcpListener) {
        let Ok((stream, addr)) = listener.accept() else {
            return;
        };
        self.handle_request(stream, addr);
        // TODO: Add multi-threading pool
    }

    /// Handle an incoming request.
    fn handle_request(&mut self, mut stream: TcpStream, addr: SocketAddr) {
        let raw_request = read_all_from_stream(&stream);
        if let Some(request) = parse_request_from_string::<()>(&raw_request) {
            if self.mode.debug_level().is_max() {
                info(
                    self.mode,
                    &format!(
                        "Request ({} bytes) from: {}\n{}",
                        raw_request.len(),
                        addr,
                        raw_request
                    ),
                );
            } else if self.mode.debug_level().is_high() {
                info(self.mode, &format!("Request from: {}", addr));
            }
            // Match the request to a route.
            if let Some((_path, mut ctx, route)) =
                self.routes.resolve(request.method(), request.uri())
            {
                let module_runtime = self.runtimes.get_mut(&route.module_path).unwrap();
                let response = match route.execute(&mut ctx, module_runtime, &self.info) {
                    Ok(response) => response,
                    Err(err) => {
                        error_code(err.message.to_string(), err.code);
                        responses::internal_server_error_default_webx(self.mode, err.message)
                    }
                };
                let http_response = serialize_response(&response);
                if let Ok(sent) = stream.write(http_response.as_bytes()) {
                    if self.mode.debug_level().is_max() {
                        info(
                            self.mode,
                            &format!("Response ({} bytes) to: {}\n{}", sent, addr, http_response),
                        );
                    } else if self.mode.debug_level().is_high() {
                        info(self.mode, &format!("Response to: {}", addr));
                    }
                } else {
                    warning(self.mode, format!("Failed to send response to: {}", addr));
                }
            } else {
                warning(
                    self.mode,
                    format!("No route match: {}", request.uri().path()),
                );
                let response = responses::not_found_default_webx(self.mode);
                let sent = stream.write(serialize_response(&response).as_bytes());
                if let Ok(n) = sent {
                    info(
                        self.mode,
                        &format!("{} response ({} bytes) to: {}", response.status(), n, addr),
                    );
                } else {
                    warning(
                        self.mode,
                        format!(
                            "Failed to send response to: {}\n{}",
                            addr,
                            sent.unwrap_err()
                        ),
                    );
                }
            }
            stream.flush().unwrap();
        } else {
            warning(self.mode, format!("Invalid request: {}", addr));
        }
    }
}
