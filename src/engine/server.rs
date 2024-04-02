use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{mpsc::Sender, Arc},
};

use http_body_util::Full;
use hyper::{
    body::{Bytes, Incoming},
    server::conn::http1,
    service::Service,
    Request, Response,
};
use hyper_util::rt::TokioIo;

use crate::{file::project::ProjectConfig, reporting::debug::info, runner::WXMode};

use super::runtime::{WXRuntimeError, WXRuntimeMessage};

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
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = tokio::net::TcpListener::bind(&self.addrs()[..]).await?;
        let svc = WXSvc::new(self.mode, self.runtime_tx.clone());
        self.log_startup();
        // Multi-threading pool via asynchronous tokio worker threads.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()?;
        loop {
            let (stream, addr) = listener.accept().await?;
            let io = TokioIo::new(stream);
            runtime.spawn(Self::serve(io, svc.clone_with_address(addr)));
        }
    }

    /// Serves a single connection.
    /// This is the main entry point for each connection to the server
    /// and simply passes the connection to the request handler `WXSvc` service.
    async fn serve(io: TokioIo<tokio::net::TcpStream>, svc: WXSvc) {
        let addr = svc.address.unwrap();
        if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
            eprintln!("failed to serve connection {}: {:?}", addr, err);
        }
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
        // Send the actor RPC request via channels to the runtime.
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.runtime_tx
            .send(WXRuntimeMessage::ExecuteRoute {
                request: req,
                addr: self.address.unwrap(),
                respond_to: tx,
            })
            .unwrap();
        Box::pin(async move { rx.await.unwrap() })
    }
}
