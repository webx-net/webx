use std::{sync::mpsc::Receiver, net::{SocketAddr, TcpListener, TcpStream}, io::{Read, Write, BufReader, BufRead}, collections::HashMap};

use http::{Response, response};

use crate::{file::webx::{WXModule, WXModulePath, WXRouteMethod, WXUrlPath, WXBody, WXRoute, WXROUTE_METHODS}, runner::WXMode, reporting::{debug::info, warning::warning, error::{error_code, ERROR_DUPLICATE_ROUTE}}, analysis::routes::{extract_flat_routes, extract_duplicate_routes, analyse_duplicate_routes, verify_model_routes}};

use super::http::{parse_request, parse_request_tcp, serialize_response};

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

}

/// This is a map of all routes in the project.
/// The key is the route path, and the value is the route.
/// This map requires that **all routes are unique**.
/// This is enforced by the `analyse_module_routes` function.
#[derive(Debug)]
pub struct WXRouteMap(HashMap<WXRouteMethod, HashMap<WXUrlPath, WXRTRoute>>);

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
        let mut route_map = HashMap::new();
        // Prepare the route map with empty method maps.
        for method in WXROUTE_METHODS {
            route_map.insert(method.clone(), HashMap::new());
        }
        // Insert all routes into each method map category.
        for ((route, path), _) in routes.unwrap().iter() {
            let method_map = route_map.get_mut(&route.method).unwrap();
            method_map.insert(path.clone(), Self::compile_route(route)?);
        }
        Ok(WXRouteMap(route_map))
    }
    
    /// Compile a parsed route into a runtime route.
    fn compile_route(route: &WXRoute) -> Result<WXRTRoute, WXRuntimeError> {
        let body = route.body.clone();
        Ok(WXRTRoute { body })
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
            mode
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
            Err(err) => error_code(format!("(Runtime) {}", err.message), err.code)
        }
    }

    /// Main runtime loop.
    /// This function will run forever in a dedicated thread
    /// and will handle all incoming requests and responses
    /// until the program is terminated.
    pub fn run(&mut self) {
        self.recompile_routes(); // Ensure that we have a valid route map.
        dbg!(&self.routes); // TODO: Remove this line (debug only
        info(self.mode, "WebX server is running!");
        let addrs = [
            SocketAddr::from(([127, 0, 0, 1], 8080)), // TODO: Only in dev mode
            SocketAddr::from(([127, 0, 0, 1], 80)),   // TODO: Only in prod mode
            SocketAddr::from(([127, 0, 0, 1], 443)),  // TODO: Only in prod mode
        ];
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
                    info(self.mode, &format!("(Runtime) New module: {}", module.path.module_name()));
                    self.source_modules.push(module);
                    self.recompile_routes();
                },
                WXRuntimeMessage::SwapModule(path, module) => {
                    info(self.mode, &format!("(Runtime) Reloaded module: {}", module.path.module_name()));
                    self.source_modules.retain(|m| m.path != path);
                    self.source_modules.push(module);
                    self.recompile_routes();
                },
                WXRuntimeMessage::RemoveModule(path) => {
                    info(self.mode, &format!("(Runtime) Removed module: {}", path.module_name()));
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
        // let mut buf = [0; 1024];
        // if let Ok(_) = stream.read(&mut buf) {
        //     info(self.mode, &format!("(Runtime) Request from: {}\n{}", addr, String::from_utf8_lossy(&buf)));
        //     let response = b"HTTP/1.1 200 OK\r\n\r\nHello, world!";
        //     stream.write(response).unwrap();
        //     stream.flush().unwrap();
        // } else { warning(format!("(Runtime) Request read failure: {}", addr)); }
        if let Some(request) = parse_request_tcp::<()>(&stream) {
            info(self.mode, &format!("(Runtime) Request from: {}\n{:#?}", stream.peer_addr().unwrap(), &request));
            let response = Response::builder().status(200).body("Hello, world!").unwrap();
            stream.write(serialize_response(&response).as_bytes()).unwrap();
            info(self.mode, &format!("(Runtime) Response to: {}\n{:#?}", stream.peer_addr().unwrap(), &response));
        } else {
            warning(format!("(Runtime) Request read failure: {}", addr));
        }
    }
}
        