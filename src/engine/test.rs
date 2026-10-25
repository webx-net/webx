//! HTTP integration tests use real loopback sockets and join every worker on drop.
#![cfg(test)]

use std::{
    fs,
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Sender},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

use http_body_util::{BodyExt, Full};
use hyper::{body::Bytes, Request, StatusCode};
use hyper_util::rt::TokioIo;

use crate::{
    engine::{
        runtime::{WXRuntime, WXRuntimeInfo, WXRuntimeMessage},
        server::WXServer,
    },
    file::{parser::parse_webx_file, project::ProjectConfig},
    runner::{DebugLevel, WXMode},
};

static NEXT_PROJECT: AtomicUsize = AtomicUsize::new(0);

struct TestProject(PathBuf);

impl TestProject {
    fn new(files: &[(&str, &str)]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "webx-test-{}-{}",
            std::process::id(),
            NEXT_PROJECT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        let project = Self(path);
        for (name, contents) in files {
            project.write(name, contents);
        }
        project
    }

    fn write(&self, name: &str, contents: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct TestServer {
    addr: SocketAddr,
    running: Arc<AtomicBool>,
    tx: Sender<WXRuntimeMessage>,
    workers: Vec<JoinHandle<()>>,
    // Dropped after all workers have joined.
    project: TestProject,
}

struct TestResponse {
    status: StatusCode,
    headers: hyper::HeaderMap,
    body: String,
}

impl TestServer {
    fn start(files: &[(&str, &str)]) -> Self {
        let project = TestProject::new(files);
        let modules = files
            .iter()
            .filter(|(name, _)| name.ends_with(".wx"))
            .map(|(name, _)| parse_webx_file(&project.0.join(name)).unwrap())
            .collect();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let (tx, rx) = mpsc::channel();
        let info = WXRuntimeInfo::new(&project.0);
        let mode = WXMode::Dev(DebugLevel::Low);
        let rt_running = running.clone();
        let runtime_worker = std::thread::spawn(move || {
            let mut runtime = WXRuntime::new(rx, mode, info);
            runtime.load_modules(modules);
            runtime.run(rt_running);
        });
        let server_tx = tx.clone();
        let server_running = running.clone();
        let server_worker = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let config: ProjectConfig =
                    serde_json::from_str(r#"{"name":"test","version":"1","port":0}"#).unwrap();
                let server = WXServer::new(mode, config, server_tx);
                server
                    .run_with_listener(
                        tokio::net::TcpListener::from_std(listener).unwrap(),
                        server_running,
                    )
                    .await
                    .unwrap();
            });
        });
        Self {
            addr,
            running,
            tx,
            workers: vec![server_worker, runtime_worker],
            project,
        }
    }

    async fn request(&self, method: &str, path: &str) -> TestResponse {
        request(self.addr, method, path).await
    }
}

async fn request(addr: SocketAddr, method: &str, path: &str) -> TestResponse {
    tokio::time::timeout(Duration::from_secs(5), async {
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut client, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
            .await
            .unwrap();
        let connection = tokio::spawn(connection);
        let response = client
            .send_request(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("Host", "localhost")
                    .header("Connection", "close")
                    .body(Full::new(Bytes::new()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let (parts, body) = response.into_parts();
        let body = body.collect().await.unwrap().to_bytes();
        drop(client);
        connection.await.unwrap().unwrap();
        TestResponse {
            status: parts.status,
            headers: parts.headers,
            body: String::from_utf8(body.to_vec()).unwrap(),
        }
    })
    .await
    .expect("HTTP request did not complete within five seconds")
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = self.tx.send(WXRuntimeMessage::Shutdown);
        for worker in self.workers.drain(..) {
            let result = worker.join();
            if !std::thread::panicking() {
                result.expect("server worker panicked");
            }
        }
    }
}

#[tokio::test]
async fn serves_html_and_reports_missing_routes_over_http() {
    let server = TestServer::start(&[("index.wx", "get / (<h1>Hello WebX</h1>)\n")]);
    let response = server.request("GET", "/").await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.headers["content-type"], "text/html; charset=utf-8");
    assert_eq!(response.body, "<h1>Hello WebX</h1>");
    assert_eq!(
        response.headers["content-length"],
        response.body.len().to_string()
    );
    let missing = server.request("GET", "/missing").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert!(missing.body.contains("404 Not Found"));
    assert_eq!(
        server.request("DELETE", "/").await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn serves_static_files_and_handler_errors_over_http() {
    let server = TestServer::start(&[
        (
            "index.wx",
            "get / -> static(\"page.html\")\nget /missing -> static(\"absent.html\")\n",
        ),
        ("page.html", "<p>Static content</p>"),
    ]);
    assert_eq!(
        server.request("GET", "/").await.body,
        "<p>Static content</p>"
    );
    let error = server.request("GET", "/missing").await;
    assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(error.body.contains("failed to read file"));
    assert_eq!(server.request("GET", "/").await.status, StatusCode::OK);
}
