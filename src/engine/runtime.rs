use std::{
    collections::HashMap,
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::mpsc::Receiver,
};

use http::{request, Method, Response};

use crate::{
    analysis::routes::verify_model_routes,
    file::webx::{WXBody, WXBodyType, WXModule, WXModulePath, WXRoute, WXUrlPath},
    reporting::{debug::info, error::error_code, warning::warning},
    runner::WXMode,
};

use super::http::{
    parse_request_tcp, serialize_response,
    Responses::{self, ok_html},
};

/// A runtime error.
pub struct WXRuntimeError {
    pub code: i32,
    pub message: String,
}

/// A runtime flat-route.
#[derive(Debug)]
pub struct WXRTRoute {
    // TODO: Add support for:
    // TODO: - handler functions
    // TODO: - global typescript code
    // TODO: - models ORM and types
    body: Option<WXBody>,
}

impl WXRTRoute {
    /// Execute the route and return a HTTP response.
    ///
    /// ## Note
    /// This function will **not** check if the route is valid.
    fn execute(&self) -> Result<Response<String>, WXRuntimeError> {
        if let Some(body) = &self.body {
            Ok(match body.body_type {
                WXBodyType::TS => {
                    todo!("(Runtime) TS body type is not supported yet");
                }
                WXBodyType::TSX => {
                    // Assume that the body is valid JSX without dynamic content.
                    // That is, the body is a static HTML page or fragment.
                    ok_html(body.body.clone())
                }
            })
        } else {
            // TODO: Add support for handlers as well.
            Err(WXRuntimeError {
                code: 500,
                message: "Route body is empty".into(),
            })
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
        Ok(WXRTRoute { body })
    }

    /// Get a route from the route map.
    /// This function will return `None` if the route does not exist.
    ///
    /// ## Note
    /// This function will **not** check for duplicate routes.
    /// This is done in the `analyse_module_routes` function.
    fn resolve(&self, method: &Method, path: &str) -> Option<(&WXUrlPath, &WXRTRoute)> {
        let routes = self.0.get(method)?;
        // Sort all routes by path length in descending order.
        // This is required to ensure that the most specific routes are matched first.
        let mut routes: Vec<(&WXUrlPath, &WXRTRoute)> = routes.iter().collect();
        routes.sort_by(|(a, _), (b, _)| b.segments().cmp(&a.segments()));
        // Go through all routes and try to match the path.
        for (route_path, route) in routes {
            dbg!("Checking", route_path, route, route_path.matches(path));
            if route_path.matches(path) {
                return Some((route_path, route));
            }
        }
        None
    }
}

/// Channel message for the runtime.
pub enum WXRuntimeMessage {
    NewModule(WXModule),
    SwapModule(WXModulePath, WXModule),
    RemoveModule(WXModulePath),
}

/// The WebX runtime.
pub struct WXRuntime {
    source_modules: Vec<WXModule>,
    routes: WXRouteMap,
    messages: Receiver<WXRuntimeMessage>,
    mode: WXMode,
}

impl WXRuntime {
    pub fn new(rx: Receiver<WXRuntimeMessage>, mode: WXMode) -> Self {
        WXRuntime {
            source_modules: Vec::new(),
            routes: WXRouteMap::new(),
            messages: rx,
            mode,
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
            Err(err) => error_code(format!("(Runtime) {}", err.message), err.code),
        }
        if self.mode == WXMode::Dev {
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
        let ports = if self.mode == WXMode::Dev {
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
        listener.set_nonblocking(self.mode == WXMode::Dev).unwrap();
        loop {
            self.listen_for_requests(&listener);
            // In dev mode, we don't want the TCP listener to block the thread.
            // Instead, we want to shortly sleep, then check for new messages
            // from the channel to enable module hotswapping.
            if self.mode == WXMode::Dev {
                self.sync_channel_messages();
                std::thread::sleep(std::time::Duration::from_millis(100));
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
                        &format!("(Runtime) New module: {}", module.path.module_name()),
                    );
                    self.source_modules.push(module);
                    self.recompile_routes();
                }
                WXRuntimeMessage::SwapModule(path, module) => {
                    info(
                        self.mode,
                        &format!("(Runtime) Reloaded module: {}", module.path.module_name()),
                    );
                    self.source_modules.retain(|m| m.path != path);
                    self.source_modules.push(module);
                    self.recompile_routes();
                }
                WXRuntimeMessage::RemoveModule(path) => {
                    info(
                        self.mode,
                        &format!("(Runtime) Removed module: {}", path.module_name()),
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
            info(
                self.mode,
                &format!("(Runtime) Request from: {}\n{:#?}", addr, &request),
            );
            // Match the request to a route.
            let url = request.uri().path();
            if let Some((path, route)) = self.routes.resolve(request.method(), url) {
                info(
                    self.mode,
                    &format!(
                        "(Runtime) Route: {} {}, matches '{}'",
                        request.method(),
                        path,
                        url
                    ),
                );
                let response = match route.execute() {
                    Ok(response) => response,
                    Err(err) => {
                        error_code(format!("(Runtime) {}", err.message), err.code);
                        Responses::internal_server_error_default_webx(self.mode, err.message)
                    }
                };
                stream
                    .write(serialize_response(&response).as_bytes())
                    .unwrap();
                info(
                    self.mode,
                    &format!("(Runtime) Response to: {}\n{:#?}", addr, &response),
                );
                info(
                    self.mode,
                    &format!("(Runtime) Response body:\n{}", &response.body()),
                );
            } else {
                warning(format!("(Runtime) Route not found: {}", url));
                stream
                    .write(
                        serialize_response(&Responses::not_found_default_webx(self.mode))
                            .as_bytes(),
                    )
                    .unwrap();
            }
            stream.flush().unwrap();
        } else {
            warning(format!("(Runtime) Request read failure: {}", addr));
        }
    }
}
