use std::{
    borrow::Borrow,
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
    v8::{self, GetPropertyNamesArgs, Local, Value},
    JsRuntime, RuntimeOptions,
};
use hyper::body::Bytes;

use crate::{
    analysis::routes::verify_model_routes,
    file::webx::{
        WXBody, WXBodyType, WXLiteralValue, WXModule, WXModulePath, WXRoute, WXRouteHandler,
        WXTypedIdentifier, WXUrlPath, WXUrlPathSegment,
    },
    reporting::{
        debug::info,
        error::{error_code, exit_error, DateTimeSpecifier},
        warning::warning,
    },
    runner::WXMode,
    timeout_duration,
};

use super::{
    http::responses::{self, ok_html},
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

trait AssertSendSync: Send + Sync + 'static {}
impl AssertSendSync for WXRuntimeError {}

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
    pub fn to_js_string(&self) -> String {
        match self {
            WXRTValue::String(s) => format!("\"{}\"", s),
            WXRTValue::Number(f) => f.to_string(),
            WXRTValue::Boolean(b) => b.to_string(),
            WXRTValue::Null => "null".into(),
            WXRTValue::Array(arr) => {
                let mut values = Vec::new();
                for value in arr.iter() {
                    values.push(value.to_js_string());
                }
                format!("[{}]", values.join(", "))
            }
            WXRTValue::Object(obj) => {
                let mut values = Vec::new();
                for (key, value) in obj.iter() {
                    values.push(format!("{}: {}", key, value.to_js_string()));
                }
                format!("{{{}}}", values.join(", "))
            }
        }
    }

    pub fn to_js_value<'a>(&self, scope: &mut v8::HandleScope<'a>) -> Local<'a, Value> {
        match self {
            WXRTValue::String(s) => v8::String::new(scope, s).unwrap().into(),
            WXRTValue::Number(f) => v8::Number::new(scope, *f).into(),
            WXRTValue::Boolean(b) => v8::Boolean::new(scope, *b).into(),
            WXRTValue::Null => v8::null(scope).into(),
            WXRTValue::Array(arr) => {
                let mut values = Vec::new();
                for value in arr.iter() {
                    values.push(value.to_js_value(scope));
                }
                let arr = v8::Array::new(scope, values.len() as i32);
                for (i, value) in values.into_iter().enumerate() {
                    arr.set_index(scope, i as u32, value);
                }
                arr.into()
            }
            WXRTValue::Object(obj) => {
                let js_obj = v8::Object::new(scope);
                for (key, value) in obj.iter() {
                    let key = v8::String::new(scope, key).unwrap();
                    let value = value.to_js_value(scope);
                    js_obj.set(scope, key.into(), value);
                }
                js_obj.into()
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
        let js_args = args.iter().map(WXRTValue::to_js_string).collect::<Vec<_>>();
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
    fn get_url_segments(url: &hyper::Uri) -> Vec<&str> {
        url.path()
            .split('/')
            .skip(1)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    }

    pub fn matches(&self, url: &hyper::Uri) -> WXPathResolution {
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
/// A runtime flat-route.
#[derive(Debug, Clone)]
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
            WXBodyType::Ts => todo!("TS body type is not supported yet"),
            // TODO: Resolve bindings, render and execute JSX (dynamic)
            WXBodyType::Tsx => Ok(WXRTValue::String(body.body.clone())),
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
    ) -> Result<hyper::Response<String>, WXRuntimeError> {
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
type WXRouteMapInner = HashMap<hyper::Method, WXMethodMapInner>;

/// This is a map of all routes in the project.
/// The key is the route path, and the value is the route.
/// This map requires that **all routes are unique**.
/// This is enforced by the `analyse_module_routes` function.
#[derive(Debug, Clone)]
pub struct WXRouteMap(WXRouteMapInner);

impl WXRouteMap {
    fn new() -> Self {
        WXRouteMap(HashMap::new())
    }

    /// Create a new route map from a list of modules.
    fn from_modules(modules: &[WXModule]) -> Result<Self, WXRuntimeError> {
        let routes = verify_model_routes(modules);
        if let Err((message, code)) = routes {
            return Err(WXRuntimeError { code, message });
        }
        let mut route_map: WXRouteMapInner = HashMap::new();
        // Insert all routes into each method map category.
        for ((route, path), _) in routes.unwrap().iter() {
            let method_map = route_map.entry(route.method.clone()).or_default();
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
    Swap(WXModulePath, WXModule),
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
    runtimes: HashMap<WXModulePath, deno_core::JsRuntime>,
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

    pub fn error_date_specifier(&self) -> DateTimeSpecifier {
        if self.mode.debug_level().is_high() {
            DateTimeSpecifier::Verbose
        } else {
            DateTimeSpecifier::Short
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
                    self.mode.debug_level().is_max(),
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
                self.mode.debug_level().is_max(),
            );
        }
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
            Err(err) => error_code(err.message, err.code, self.error_date_specifier()),
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
                println!(" - {} {}", method, path);
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
    /// - Hotswapp module in dev mode.
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
                        self.remove_module(&path);
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
                    WXRuntimeMessage::ExecuteRoute {
                        request,
                        addr,
                        respond_to,
                    } => respond_to.send(self.execute_route(request, addr)).unwrap(),
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
            let module_runtime = self.runtimes.get_mut(&route.module_path).unwrap();
            let route_result = route.execute(&mut ctx, module_runtime, &self.info);
            let response = match route_result {
                Ok(response) => response,
                Err(err) => {
                    error_code(
                        err.message.to_string(),
                        err.code,
                        self.error_date_specifier(),
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
            let response = responses::not_found_default_webx(self.mode);
            info(
                self.mode,
                &format!("{} response to: {}", response.status(), addr),
            );
            Ok(response.map(http_body_util::Full::from))
        }
    }
}
