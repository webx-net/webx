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
        Self::start_mode(files, WXMode::Dev(DebugLevel::Low))
    }

    fn start_mode(files: &[(&str, &str)], mode: WXMode) -> Self {
        let project = TestProject::new(files);
        let modules = files
            .iter()
            .filter(|(name, _)| name.ends_with(".wx") || name.ends_with(".webx"))
            .map(|(name, _)| parse_webx_file(&project.0.join(name)).unwrap())
            .collect();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let (tx, rx) = mpsc::channel();
        let info = WXRuntimeInfo::new(&project.0);
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

#[tokio::test]
async fn route_specificity_is_independent_of_declaration_and_module_order() {
    for reverse in [false, true] {
        let routes = [
            "get /path/(anything: String)/here (<p>generic</p>)",
            "get /path/to/here (<p>literal</p>)",
            "get /path/*/here (<p>wildcard</p>)",
            "location /nested { get /(id: String) (<p>nested generic</p>) get /fixed (<p>nested literal</p>) }",
            "get /other/* (<p>fallback</p>)",
            "delete /path/to/here (<p>deleted</p>)",
        ];
        let source = if reverse {
            routes.into_iter().rev().collect::<Vec<_>>()
        } else {
            routes.to_vec()
        }
        .join("\n");
        let files = if reverse {
            vec![
                ("specific.wx", "get /other/fixed (<p>other literal</p>)\n"),
                ("generic.wx", source.as_str()),
            ]
        } else {
            vec![
                ("generic.wx", source.as_str()),
                ("specific.wx", "get /other/fixed (<p>other literal</p>)\n"),
            ]
        };
        let server = TestServer::start(&files);
        for _ in 0..4 {
            for (path, expected) in [
                ("/path/to/here", "<p>literal</p>"),
                ("/path/else/here", "<p>generic</p>"),
                ("/nested/fixed", "<p>nested literal</p>"),
                ("/nested/123", "<p>nested generic</p>"),
                ("/other/fixed", "<p>other literal</p>"),
                ("/other/anything", "<p>fallback</p>"),
            ] {
                let response = server.request("GET", path).await;
                assert_eq!(response.status, StatusCode::OK);
                assert_eq!(response.body, expected, "{path}");
            }
        }
        assert_eq!(
            server.request("GET", "/path/to/here/extra").await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            server.request("GET", "/nested").await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            server.request("DELETE", "/path/to/here").await.body,
            "<p>deleted</p>"
        );
    }
}

#[test]
fn duplicate_and_ambiguous_routes_report_both_locations() {
    use crate::analysis::routes::{
        ambiguous_route_warnings, analyze_duplicate_routes, analyze_invalid_routes,
        extract_flat_routes,
    };
    for (a, b) in [
        ("get /same (<p>a</p>)", "get /same (<p>b</p>)"),
        (
            "get /item/(a: String) (<p>a</p>)",
            "get /item/(b: Int) (<p>b</p>)",
        ),
        (
            "get /nested/same (<p>a</p>)",
            "location /nested { get /same (<p>b</p>) }",
        ),
    ] {
        let project = TestProject::new(&[("a.wx", a), ("b.wx", b)]);
        let modules = ["a.wx", "b.wx"].map(|name| parse_webx_file(&project.0.join(name)).unwrap());
        let err = analyze_duplicate_routes(&modules).unwrap_err();
        assert!(err.message.contains("Duplicate/ambiguous"));
        assert!(err.message.contains("a line"), "{}", err.message);
        assert!(err.message.contains("b line"), "{}", err.message);
    }
    let project = TestProject::new(&[(
        "cross.wx",
        "get /fixed/(x: String) (a)\nget /(y: String)/fixed (b)\npost /empty (ok)\n",
    )]);
    let modules = [parse_webx_file(&project.0.join("cross.wx")).unwrap()];
    assert_eq!(
        ambiguous_route_warnings(&extract_flat_routes(&modules)).len(),
        1
    );
    analyze_invalid_routes(&modules).unwrap();
}

#[tokio::test]
async fn edited_html_is_visible_on_the_very_next_request() {
    let server = TestServer::start(&[("index.wx", "get / (<h1>before</h1>)\n")]);
    assert_eq!(server.request("GET", "/").await.body, "<h1>before</h1>");
    // No sleep or watcher polling: the completed save precedes the request.
    for text in ["after", "again", "final", "Hej 🌍 — café"] {
        server
            .project
            .write("index.wx", &format!("get / (<h1>{text}</h1>)\n"));
        assert_eq!(
            server.request("GET", "/").await.body,
            format!("<h1>{text}</h1>")
        );
    }
    server
        .project
        .write("save.tmp", "get / (<h1>atomic save</h1>)\n");
    fs::rename(
        server.project.0.join("save.tmp"),
        server.project.0.join("index.wx"),
    )
    .unwrap();
    assert_eq!(
        server.request("GET", "/").await.body,
        "<h1>atomic save</h1>"
    );
}

#[tokio::test]
async fn reload_adds_removes_and_renames_modules_and_keeps_last_good_snapshot() {
    let source = "global { function count() { return 7; } }\nget / -> count()\n";
    let server = TestServer::start(&[("index.wx", source)]);
    assert_eq!(server.request("GET", "/").await.body, "7");
    for bad in [
        "get / (unfinished",
        "get / (a)\nget / (b)\n",
        "global { this is invalid JS; }\nget / (broken)\n",
    ] {
        server.project.write("index.wx", bad);
        assert_eq!(server.request("GET", "/").await.body, "7", "{bad}");
    }
    server.project.write("index.wx", source);
    server
        .project
        .write("nested/added.webx", "get /added (added)\n");
    assert_eq!(server.request("GET", "/added").await.body, "added");
    fs::rename(
        server.project.0.join("nested/added.webx"),
        server.project.0.join("renamed.wx"),
    )
    .unwrap();
    assert_eq!(server.request("GET", "/added").await.body, "added");
    fs::remove_file(server.project.0.join("renamed.wx")).unwrap();
    assert_eq!(
        server.request("GET", "/added").await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(server.request("GET", "/").await.body, "7");
}

#[tokio::test]
async fn html_only_reload_preserves_globals_and_changed_globals_reset_them() {
    let source = "global { let n = 0; function count() { return ++n; } }\nget / -> count()\nget /page (before)\n";
    let server = TestServer::start(&[("index.wx", source)]);
    assert_eq!(server.request("GET", "/").await.body, "1");
    server
        .project
        .write("index.wx", &source.replace("before", "after"));
    assert_eq!(server.request("GET", "/page").await.body, "after");
    assert_eq!(server.request("GET", "/").await.body, "2");
    server
        .project
        .write("index.wx", &source.replace("n = 0", "n = 100"));
    assert_eq!(server.request("GET", "/").await.body, "101");
}

#[tokio::test]
async fn production_does_not_reload_modules_but_static_files_are_read_each_time() {
    let source = "get / (before)\nget /file -> static(\"page.html\")\n";
    let server = TestServer::start_mode(
        &[("index.wx", source), ("page.html", "before")],
        WXMode::Prod,
    );
    assert_eq!(server.request("GET", "/").await.body, "before");
    server.project.write("index.wx", "get / (after)\n");
    server.project.write("page.html", "after");
    assert_eq!(server.request("GET", "/").await.body, "before");
    assert_eq!(server.request("GET", "/file").await.body, "after");
}
