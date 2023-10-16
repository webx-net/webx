use std::{
    collections::HashMap,
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::mpsc::Receiver, path::{Path, PathBuf},
};

use http::{Method, Response, Uri};

use crate::{
    analysis::routes::verify_model_routes,
    file::webx::{WXBody, WXBodyType, WXModule, WXModulePath, WXRoute, WXUrlPath, WXPathResolution, WXPathBindings, WXRouteHandler, WXLiteralValue},
    reporting::{debug::info, error::error_code, warning::warning},
    runner::WXMode,
};

use super::{http::{
    parse_request_tcp, serialize_response,
    Responses::{self, ok_html},
};

/// A runtime error.
pub struct WXRuntimeError {
    pub code: i32,
    pub message: String,
}

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
        self.values.get(ident).map(|v| v.clone())
    }
}

/// Runtime values in WebX.
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum WXRTValue {
    String(String),
    Number(u32, u32),
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
            WXRTValue::Number(i, d) => format!("{}{}", i, d),
            WXRTValue::Boolean(b) => format!("{}", b),
            WXRTValue::Null => "null".into(),
            WXRTValue::Array(arr) => {
                let mut values = Vec::new();
                for value in arr.iter() {
                    values.push(value.to_js());
                }
                format!("[{}]", values.join(", "))
            },
            WXRTValue::Object(obj) => {
                let mut values = Vec::new();
                for (key, value) in obj.iter() {
                    values.push(format!("{}: {}", key, value.to_js()));
                }
                format!("{{{}}}", values.join(", "))
            },
        }
    }

    /// Convert the runtime value into a raw value string.
    /// This function will **not** wrap strings in quotes.
    /// This function is used for sanitizing values in JSX render functions to be sent to the client.
    /// This function will **not** escape any characters.
    pub fn to_raw(&self) -> String {
        match self {
            WXRTValue::String(s) => s.clone(),
            WXRTValue::Number(i, d) => format!("{}{}", i, d),
            WXRTValue::Boolean(b) => format!("{}", b),
            WXRTValue::Null => "null".into(),
            WXRTValue::Array(arr) => {
                let mut values = Vec::new();
                for value in arr.iter() {
                    values.push(value.to_raw());
                }
                format!("[{}]", values.join(", "))
            },
            WXRTValue::Object(obj) => {
                let mut values = Vec::new();
                for (key, value) in obj.iter() {
                    values.push(format!("{}: {}", key, value.to_raw()));
                }
                format!("{{{}}}", values.join(", "))
            },
        }
    }
}

fn eval_literal(literal: &WXLiteralValue, ctx: &WXRTContext) -> Result<WXRTValue, WXRuntimeError> {
    match literal {
        WXLiteralValue::String(s) => Ok(WXRTValue::String(s.clone())),
        WXLiteralValue::Number(i, d) => Ok(WXRTValue::Number(*i, *d)),
        WXLiteralValue::Boolean(b) => Ok(WXRTValue::Boolean(*b)),
        WXLiteralValue::Null => Ok(WXRTValue::Null),
        WXLiteralValue::Array(arr) => {
            let mut values = Vec::new();
            for value in arr.iter() {
                values.push(eval_literal(value, ctx)?);
            }
            Ok(WXRTValue::Array(values))
        },
        WXLiteralValue::Object(obj) => {
            let mut values = Vec::new();
            for (key, value) in obj.iter() {
                values.push((key.clone(), eval_literal(value, ctx)?));
            }
            Ok(WXRTValue::Object(values))
        },
        WXLiteralValue::Identifier(ident) => {
            if let Some(value) = ctx.resolve(&ident) {
                Ok(value)
            } else {
                Err(WXRuntimeError {
                    code: 500,
                    message: format!("Identifier '{}' not found in context", ident),
                })
            }
        },
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
    fn execute(&self, ctx: &WXRTContext, info: &WXRuntimeInfo) -> Result<WXRTValue, WXRuntimeError> {
        let args = self.args.iter().map(|arg| eval_literal(arg, &ctx)).collect::<Result<Vec<_>, _>>()?;
        // TODO: Add support for custom user-defined handlers
        Err(WXRuntimeError {
            code: 500,
            message: format!("Handler '{}' not found", self.name),
        })
    }
}


/// A runtime flat-route.
#[derive(Debug)]
pub struct WXRTRoute {
    // TODO: Add support for:
    // TODO: - handler functions
    // TODO: - global typescript code
    // TODO: - models ORM and types
    body: Option<WXBody>,
    pre_handlers: Vec<WXRTHandlerCall>,
    post_handlers: Vec<WXRTHandlerCall>,
}

impl WXRTRoute {
    fn execute_body(&self) -> Result<WXRTValue, WXRuntimeError> {
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
    fn execute(&self, info: &WXRuntimeInfo) -> Result<Response<String>, WXRuntimeError> {
        // TODO: Refactor this function to combine all logic into a better structure.
        if self.pre_handlers.len() == 0 && self.body.is_none() && self.post_handlers.len() == 0 {
            // No handlers or body are present, return an empty response.
            return Err(WXRuntimeError {
                code: 500,
                message: "Route is empty".into(),
            });
        } else if self.pre_handlers.len() == 0 && self.body.is_some() && self.post_handlers.len() == 0 {
            // Only a body is present, execute it and return the result.
            Ok(ok_html(self.execute_body()?.to_raw()))
        } else if self.body.is_none() {
            // Only handlers are present, execute them sequentially.
            // Merge all pre and post handlers into a single handler vector.
            let mut handlers = self.pre_handlers.clone();
            handlers.extend(self.post_handlers.clone());
            // Execute all (but last() handlers sequentially.
            let mut ctx = WXRTContext::new();
            for handler in handlers.iter().take(handlers.len() - 1) {
                let result = handler.execute(&ctx, info)?;
                // Bind the result to the output variable.
                if let Some(output) = &handler.output { ctx.bind(output, result); }
            }
            // Execute the last handler and return the result as the response.
            let handler = handlers.last().unwrap();
            Ok(ok_html(handler.execute(&ctx, info)?.to_raw()))
        } else {
            // Both handlers and a body are present, execute them sequentially.
            todo!("Handlers + body is not supported yet");
            /*
                // TODO: Execute pre-handlers
                let body = self.execute_body()?;
                // TODO: Execute post-handlers, pass body result in ctx
                if body.is_none() {
                    return Err(WXRuntimeError {
                        code: 500,
                        message: "Route body is empty".into(),
                    });
                }
                Ok(body.unwrap())
            */
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
                .or_insert(HashMap::new());
            method_map.insert(path.clone(), Self::compile_route(route)?);
        }
        Ok(WXRouteMap(route_map))
    }

    /// Compile a parsed route into a runtime route.
    fn compile_route(route: &WXRoute) -> Result<WXRTRoute, WXRuntimeError> {
        let body = route.body.clone();
        Ok(WXRTRoute {
            body,
            pre_handlers: route.pre_handlers.iter().map(WXRTHandlerCall::from_handler).collect(),
            post_handlers: route.post_handlers.iter().map(WXRTHandlerCall::from_handler).collect(),
        })
    }

    /// Get a route from the route map.
    /// This function will return `None` if the route does not exist.
    ///
    /// ## Note
    /// This function will **not** check for duplicate routes.
    /// This is done in the `analyse_module_routes` function.
    fn resolve(&self, method: &Method, path: &Uri) -> Option<(&WXUrlPath, WXPathBindings, &WXRTRoute)> {
        let routes = self.0.get(method)?;
        // Sort all routes by path length in descending order.
        // This is required to ensure that the most specific routes are matched first.
        let mut routes: Vec<(&WXUrlPath, &WXRTRoute)> = routes.iter().collect();
        routes.sort_by(|(a, _), (b, _)| b.segments().cmp(&a.segments()));
        // Go through all routes and try to match the path.
        let mut best_match = None;
        for (route_path, route) in routes {
            // dbg!("Checking", route_path, route, route_path.matches(path));
            match route_path.matches(path) {
                WXPathResolution::None => continue,
                WXPathResolution::Perfect(bindings) => {
                    best_match = Some((route_path, bindings, route));
                    break;
                },
                WXPathResolution::Partial(bindings) => {
                    best_match = Some((route_path, bindings, route));
                },
            }
        }
        best_match
    }
}

/// Channel message for the runtime.
pub enum WXRuntimeMessage {
    NewModule(WXModule),
    SwapModule(WXModulePath, WXModule),
    RemoveModule(WXModulePath),
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
    source_modules: Vec<WXModule>,
    routes: WXRouteMap,
    messages: Receiver<WXRuntimeMessage>,
    mode: WXMode,
    info: WXRuntimeInfo,
}

impl WXRuntime {
    pub fn new(rx: Receiver<WXRuntimeMessage>, mode: WXMode, info: WXRuntimeInfo) -> Self {
        WXRuntime {
            source_modules: Vec::new(),
            routes: WXRouteMap::new(),
            messages: rx,
            mode,
            info,
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
        self.source_modules.extend(modules);
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
    fn recompile_routes(&mut self) {
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

    /// Main runtime loop.
    /// This function will run forever in a dedicated thread
    /// and will handle all incoming requests and responses
    /// until the program is terminated.
    pub fn run(&mut self) {
        self.recompile_routes(); // Ensure that we have a valid route map.
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
                WXRuntimeMessage::NewModule(module) => {
                    info(
                        self.mode,
                        &format!("New module: {}", module.path.module_name()),
                    );
                    self.source_modules.push(module);
                    self.recompile_routes();
                }
                WXRuntimeMessage::SwapModule(path, module) => {
                    info(
                        self.mode,
                        &format!("Reloaded module: {}", module.path.module_name()),
                    );
                    self.source_modules.retain(|m| m.path != path);
                    self.source_modules.push(module);
                    self.recompile_routes();
                }
                WXRuntimeMessage::RemoveModule(path) => {
                    info(
                        self.mode,
                        &format!("Removed module: {}", path.module_name()),
                    );
                    self.source_modules.retain(|m| m.path != path);
                    self.recompile_routes();
                }
            }
        }
    }

    /// Listen for incoming requests.
    ///
    /// ## Blocking
    /// This function is **non-blocking** and will return immediately depending on the
    /// value of `listener.set_nonblocking` in the `run` function.
    fn listen_for_requests(&self, listener: &TcpListener) {
        let Ok((stream, addr)) = listener.accept() else { return };
        self.handle_request(stream, addr);
        // TODO: Add multi-threading pool
    }

    /// Handle an incoming request.
    fn handle_request(&self, mut stream: TcpStream, addr: SocketAddr) {
        if let Some(request) = parse_request_tcp::<()>(&stream) {
            if self.mode.debug_level().is_max() { info(self.mode, &format!("Request from: {}\n{:#?}", addr, &request)); }
            else if self.mode.debug_level().is_high() { info(self.mode, &format!("Request from: {}", addr)); }
            // Match the request to a route.
            if let Some((path, _bindings, route)) = self.routes.resolve(request.method(), request.uri()) {
                info(
                    self.mode,
                    &format!(
                        "Route: {} {}, matches '{}'",
                        request.method(),
                        path,
                        request.uri().path()
                    ),
                );
                let response = match route.execute(&self.info) {
                    Ok(response) => response,
                    Err(err) => {
                        error_code(format!("{}", err.message), err.code);
                        Responses::internal_server_error_default_webx(self.mode, err.message)
                    }
                };
                stream
                    .write(serialize_response(&response).as_bytes())
                    .unwrap();
                if self.mode.debug_level().is_max() {
                    info(self.mode, &format!("Response to: {}\n{:#?}", addr, &response));
                    info(self.mode, &format!("Response body:\n{}", &response.body()));
                } else if self.mode.debug_level().is_high() {
                    info(self.mode, &format!("Response to: {}", addr));
                }
            } else {
                warning(self.mode, format!("No route match: {}", request.uri().path()));
                stream
                    .write(
                        serialize_response(&Responses::not_found_default_webx(self.mode))
                            .as_bytes(),
                    )
                    .unwrap();
                info(self.mode, &format!("Response to: {}", addr));
            }
            stream.flush().unwrap();
        } else {
            warning(self.mode, format!("Request read failure: {}", addr));
        }
    }
}
