use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc,
    },
};

use http_body_util::Full;
use hyper::{
    body::{Bytes, Incoming},
    server::conn::http1,
    service::Service,
    Request, Response,
};
use hyper_util::rt::TokioIo;
use tokio::time::timeout;

use crate::{
    file::project::ProjectConfig,
    reporting::{
        debug::info,
        error::{error_code, ERROR_EXEC_ROUTE},
    },
    runner::WXMode,
    timeout_duration,
};

use super::runtime::{WXRuntimeError, WXRuntimeMessage};

/// A failable type.
pub type WXFailable<T> = Result<T, WXRuntimeError>;

impl From<std::io::Error> for WXRuntimeError {
    fn from(err: std::io::Error) -> Self {
        WXRuntimeError {
            code: 500,
            message: format!("IO error: {}", err),
        }
    }
}

/// The WebX web server.
pub struct WXServer {
    mode: WXMode,
    _config: ProjectConfig,
    runtime_tx: Arc<Sender<WXRuntimeMessage>>,
}

impl WXServer {
    pub fn new(mode: WXMode, config: ProjectConfig, rt_tx: Sender<WXRuntimeMessage>) -> Self {
        WXServer {
            mode,
            _config: config,
            runtime_tx: Arc::new(rt_tx),
        }
    }

    fn ports(&self) -> Vec<u16> {
        if self.mode.is_dev() {
            vec![8080]
        } else {
            vec![80, 443]
        }
    }

    fn addrs(&self) -> Vec<std::net::SocketAddr> {
        self.ports()
            .iter()
            .map(|port| SocketAddr::from(([127, 0, 0, 1], *port)))
            .collect::<Vec<_>>()
    }

    fn log_startup(&mut self) {
        info(
            self.mode,
            &format!(
                "WebX server is listening on: {}",
                self.ports()
                    .iter()
                    .map(|p| format!("http://localhost:{}", p))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }

    /// Starts the WebX web server and listens for incoming requests in its own thread.
    pub fn run(&mut self, running: Arc<AtomicBool>) -> WXFailable<()> {
        // Multi-threading pool via asynchronous tokio worker threads.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .thread_name("webx-server")
            .enable_all()
            .build()
            // .worker_threads(4)
            .unwrap();
        runtime.block_on(self.run_async(running))?;
        runtime.shutdown_background(); // Shutdown the runtime.
        Ok(())
    }

    async fn run_async(&mut self, running: Arc<AtomicBool>) -> WXFailable<()> {
        let listener = tokio::net::TcpListener::bind(&self.addrs()[..]).await?;
        let svc = WXSvc::new(self.mode, self.runtime_tx.clone());
        self.log_startup();
        loop {
            if !running.load(Ordering::SeqCst) {
                // println!("Shutting down web server...");
                return Ok(()); // Shutdown the server.
            }
            let (stream, addr) = match timeout(timeout_duration(self.mode), listener.accept()).await
            {
                Ok(Ok((stream, addr))) => (stream, addr),
                Ok(Err(err)) => {
                    eprintln!("Failed to accept connection: {}", err);
                    continue;
                }
                Err(_) => continue,
            };
            tokio::spawn(Self::serve(
                TokioIo::new(stream),
                svc.clone_with_address(addr),
            ));
        }
    }

    /// Serves a single connection.
    /// This is the main entry point for each connection to the server
    /// and simply passes the connection to the request handler `WXSvc` service.
    async fn serve(io: TokioIo<tokio::net::TcpStream>, svc: WXSvc) -> WXFailable<()> {
        let addr = svc
            .address
            .expect("No address found while serving connection.");
        if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
            return Err(WXRuntimeError {
                code: 500,
                message: format!("failed to serve connection {}: {:?}", addr, err),
            });
        }
        Ok(())
    }
}

/// The WebX server context.
/// This is the context that is passed to each request handler.
///
/// Reference implementation: https://github.com/hyperium/hyper/blob/master/examples/service_struct_impl.rs
#[derive(Clone, Debug)]
struct WXSvc {
    mode: WXMode,
    address: Option<SocketAddr>,
    runtime_tx: Arc<Sender<WXRuntimeMessage>>,
}

impl WXSvc {
    pub fn new(mode: WXMode, rt_tx: Arc<Sender<WXRuntimeMessage>>) -> Self {
        WXSvc {
            mode,
            address: None, // Get the address from the request.
            runtime_tx: rt_tx,
        }
    }

    fn clone_with_address(&self, addr: SocketAddr) -> Self {
        let mut new = self.clone();
        new.address = Some(addr);
        new
    }

    fn _ok(&self, text: String) -> Result<Response<Full<Bytes>>, hyper::Error> {
        Ok(Response::new(Full::new(Bytes::from(text))))
    }
}

impl Service<Request<Incoming>> for WXSvc {
    type Response = Response<Full<Bytes>>;
    type Error = WXRuntimeError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    /// The WebX server request handler.
    /// This is the main entry point for all requests.
    /// It is responsible for routing requests to the appropriate handler.
    /// It is also responsible for error handling, logging, etc.
    ///
    /// ## Note
    /// - Called for each request.
    /// - Runs in a separate "thread"/tokio task.
    /// - It is asynchronous.
    /// - Respond back to the client.
    ///
    /// But most importantly, it will communicate with the WebX engine and runtimes.
    fn call(&self, req: Request<Incoming>) -> Self::Future {
        if self.mode.debug_level().is_max() {
            info(
                self.mode,
                &format!(
                    "Request from: {}\n{}",
                    self.address.unwrap(),
                    super::http::requests::serialize(&req)
                ),
            );
        } else if self.mode.debug_level().is_high() {
            info(
                self.mode,
                &format!("Request from: {}", self.address.unwrap()),
            );
        }
        let date_spec = self.mode.date_specifier();
        // Send the actor RPC request via channels to the runtime.
        let (tx, rx) = tokio::sync::oneshot::channel();
        if let Err(err) = self.runtime_tx.send(WXRuntimeMessage::ExecuteRoute {
            request: req,
            addr: self.address.unwrap(),
            respond_to: tx,
        }) {
            let error_msg = format!("Failed to execute route due to: {}", err);
            error_code(error_msg.clone(), ERROR_EXEC_ROUTE, date_spec);
            Box::pin(async move {
                Err(WXRuntimeError {
                    code: 500,
                    message: error_msg,
                })
            })
        } else {
            Box::pin(async move {
                match rx.await {
                    Ok(value) => value,
                    Err(err) => {
                        let error_msg = format!("Failed to execute route due to: {}", err);
                        error_code(error_msg.clone(), ERROR_EXEC_ROUTE, date_spec);
                        Err(WXRuntimeError {
                            code: 500,
                            message: error_msg,
                        })
                    }
                }
            })
        }
    }
}
