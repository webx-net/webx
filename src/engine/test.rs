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
    request_body(addr, method, path, "").await
}

async fn request_body(addr: SocketAddr, method: &str, path: &str, body: &str) -> TestResponse {
    request_typed(addr, method, path, body, "").await
}

async fn request_typed(
    addr: SocketAddr,
    method: &str,
    path: &str,
    body: &str,
    content_type: &str,
) -> TestResponse {
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
                    .header("Content-Type", content_type)
                    .body(Full::new(Bytes::copy_from_slice(body.as_bytes())))
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
        "post / json(title: String,",
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_requests_isolate_parameters_outputs_and_failures() {
    let source = r#"
global {
    let calls = 0;
    function echo(value) { return value; }
    function pack(token, before, middle, rendered) { return { token, before, middle, rendered, call: ++calls }; }
    function fail(value) { throw new Error(value); }
}
get /work/(token: String) -> echo(token): before, echo(before): middle { const local = middle; return local; } -> echo(out): rendered, pack(token, before, middle, rendered)
get /fail/(token: String) -> echo(token): before, fail(before) (must not run)
get /probe -> echo([typeof token, typeof before, typeof middle, typeof out, typeof rendered, typeof local])
"#;
    for mode in [WXMode::Dev(DebugLevel::Low), WXMode::Prod] {
        let server = TestServer::start_mode(&[("index.wx", source)], mode);
        let barrier = Arc::new(tokio::sync::Barrier::new(33));
        let mut requests = tokio::task::JoinSet::new();
        for i in 0..32 {
            let barrier = barrier.clone();
            let addr = server.addr;
            requests.spawn(async move {
                barrier.wait().await;
                let token = format!("request{i}");
                let response = request(addr, "GET", &format!("/work/{token}")).await;
                assert_eq!(response.status, StatusCode::OK);
                assert_eq!(response.headers["content-type"], "application/json");
                let json: serde_json::Value = serde_json::from_str(&response.body).unwrap();
                for name in ["token", "before", "middle"] {
                    assert_eq!(json[name], token);
                }
                assert_eq!(json["rendered"], token);
                let failed = request(addr, "GET", &format!("/fail/{token}")).await;
                assert_eq!(failed.status, StatusCode::INTERNAL_SERVER_ERROR);
                assert!(failed.body.contains(&token));
                json["call"].as_u64().unwrap()
            });
        }
        barrier.wait().await;
        let mut calls = Vec::new();
        while let Some(result) = requests.join_next().await {
            calls.push(result.unwrap());
        }
        calls.sort();
        // Explicit module state is shared and serialized; request bindings are not.
        assert_eq!(calls, (1..=32).collect::<Vec<_>>());
        let probe = server.request("GET", "/probe").await;
        assert_eq!(
            probe.body,
            r#"["undefined","undefined","undefined","undefined","undefined","undefined"]"#
        );
    }
}

#[tokio::test]
async fn handler_arguments_run_once_and_failed_or_unsupported_work_does_not_crash() {
    let source = r#"
global {
    let count = 0;
    function echo(value) { return value; }
    function bad() { const value = {}; value.self = value; return value; }
    function nothing() {}
    function later() { return Promise.resolve(1); }
}
get /once -> echo(++count)
get /bad -> bad()
get /nothing -> nothing()
get /async -> later()
get /code { return 1; }
post /body json(title: String) -> echo(title)
get /post (body) -> echo(out): first, echo(first): second, echo(second)
get /raw/(value: Int) -> echo(value)
post /ignored -> echo(typeof title)
"#;
    let server = TestServer::start(&[("index.wx", source)]);
    assert_eq!(server.request("GET", "/once").await.body, "1");
    assert_eq!(server.request("GET", "/once").await.body, "2");
    assert_eq!(
        server.request("GET", "/bad").await.status,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(server.request("GET", "/nothing").await.body, "null");
    assert_eq!(
        server.request("GET", "/async").await.status,
        StatusCode::NOT_IMPLEMENTED
    );
    assert_eq!(
        server.request("POST", "/body").await.status,
        StatusCode::NOT_IMPLEMENTED
    );
    assert_eq!(server.request("GET", "/code").await.body, "1");
    let unsupported = request_body(server.addr, "POST", "/body", r#"{"title":"body value"}"#).await;
    assert_eq!(unsupported.status, StatusCode::NOT_IMPLEMENTED);
    let ignored = request_body(server.addr, "POST", "/ignored", r#"{"title":"body value"}"#).await;
    assert_eq!(ignored.status, StatusCode::OK);
    assert_eq!(ignored.body, "undefined");
    assert_eq!(server.request("GET", "/post").await.body, "body");
    assert_eq!(
        server
            .request("GET", "/raw/hello%20world?unused=true")
            .await
            .body,
        "hello%20world"
    );
    assert_eq!(server.request("GET", "/once").await.body, "3");
}

fn todo_server() -> TestServer {
    TestServer::start(&[
        ("index.wx", include_str!("../../examples/todo/index.wx")),
        ("index.html", include_str!("../../examples/todo/index.html")),
    ])
}

async fn todo_fragment(server: &TestServer, method: &str, path: &str) -> String {
    let response = server.request(method, path).await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.headers["content-type"], "text/html; charset=utf-8");
    assert!(response.body.starts_with(r#"<section id="todo-panel">"#));
    assert!(response.body.contains(r#"role="status""#));
    response.body
}

async fn todo_put(server: &TestServer, path: &str, body: &str) -> String {
    let response = request_typed(
        server.addr,
        "PUT",
        path,
        body,
        "application/x-www-form-urlencoded",
    )
    .await;
    assert_eq!(response.status, StatusCode::OK, "{}", response.body);
    assert_eq!(response.headers["content-type"], "text/html; charset=utf-8");
    response.body
}

#[tokio::test]
async fn todo_example_completes_a_crud_lifecycle() {
    let server = todo_server();
    let page = server.request("GET", "/").await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains("htmx.org@2.0.8"));
    assert!(page.body.contains(r#"hx-get="/todos" hx-trigger="load""#));
    assert!(page
        .body
        .contains(r##"hx-target="#todo-panel" hx-swap="outerHTML""##));

    let empty = todo_fragment(&server, "GET", "/todos").await;
    assert!(empty.contains("No todos yet."));
    assert!(empty.contains(r#"hx-post="/todos" hx-params="none""#));

    let created = todo_fragment(&server, "POST", "/todos/First%20task").await;
    assert!(created.contains("Todo added"));
    assert!(created.contains("First task"));
    assert!(created.contains(r#"hx-get="/todos/1""#));
    assert!(created.contains(r#"hx-delete="/todos/1""#));
    assert!(!created.contains("No todos yet."));

    let edit = todo_fragment(&server, "GET", "/todos/1").await;
    assert!(edit.contains(r#"<form hx-put="/todos/1">"#));
    assert!(edit.contains(r#"value="First task""#));
    assert!(!edit.contains("checked"));

    let updated = todo_put(&server, "/todos/1", "title=Edited%2Ftask&completed=true").await;
    assert!(updated.contains("Todo updated"));
    assert!(updated.contains("Edited/task"));
    assert!(updated.contains(">Completed</span>"));
    assert!(!updated.contains("First task"));
    let listed = todo_fragment(&server, "GET", "/todos").await;
    assert!(listed.contains("Edited/task"));
    assert!(todo_fragment(&server, "GET", "/todos/1")
        .await
        .contains("checked"));
    let unchecked = todo_put(&server, "/todos/1", "title=Edited%2Ftask").await;
    assert!(unchecked.contains(">Pending</span>"));

    let deleted = todo_fragment(&server, "DELETE", "/todos/1").await;
    assert!(deleted.contains("Todo deleted"));
    assert!(deleted.contains("No todos yet."));
    assert!(!deleted.contains("Edited/task"));
    assert!(todo_fragment(&server, "GET", "/todos/1")
        .await
        .contains("Todo not found"));
    assert!(todo_fragment(&server, "GET", "/todos")
        .await
        .contains("No todos yet."));
}

#[tokio::test]
async fn todo_example_escapes_titles_and_rejects_invalid_changes() {
    let server = todo_server();
    let escaped = "&lt;img src=x onerror=alert(1)&gt;&amp;&quot;&#39; / 🌍";
    let created = todo_fragment(
        &server,
        "POST",
        "/todos/%3Cimg%20src%3Dx%20onerror%3Dalert(1)%3E%26%22%27%20%2F%20%F0%9F%8C%8D",
    )
    .await;
    assert!(created.contains(escaped));
    assert!(!created.contains("<img"));
    let edit = todo_fragment(&server, "GET", "/todos/1").await;
    assert!(edit.contains(&format!("value=\"{}\"", escaped)));
    for (method, path, message) in [
        ("POST", "/todos/%20", "Title must contain 1-120 characters"),
        ("POST", "/todos/%FF", "Title must contain 1-120 characters"),
        ("DELETE", "/todos/999", "Todo not found"),
    ] {
        assert!(todo_fragment(&server, method, path).await.contains(message));
        let stored = todo_fragment(&server, "GET", "/todos").await;
        assert!(stored.contains(escaped));
        assert!(stored.contains(">Pending</span>"));
    }
    assert!(todo_put(&server, "/todos/1", "title=%20&completed=true")
        .await
        .contains("Title must contain 1-120 characters"));
    assert!(todo_put(&server, "/todos/999", "title=Changed")
        .await
        .contains("Todo not found"));
    let rejected = request_typed(
        server.addr,
        "PUT",
        "/todos/1",
        "title=Changed&completed=notabool",
        "application/x-www-form-urlencoded",
    )
    .await;
    assert_eq!(rejected.status, StatusCode::BAD_REQUEST);
    assert!(todo_fragment(&server, "GET", "/todos")
        .await
        .contains(escaped));
    let long_title = format!("/todos/{}", "x".repeat(121));
    assert!(todo_fragment(&server, "POST", &long_title)
        .await
        .contains("Title must contain 1-120 characters"));
}

#[tokio::test]
async fn inline_endpoint_blocks_return_values_and_isolate_locals_after_errors() {
    let server = TestServer::start(&[(
        "index.wx",
        r#"
global { let calls = 0; function echo(value) { return value; } }
get /item/(id: String) -> echo(id): before {
    const local = before;
    var temporary = ++calls;
    return { id: local, call: temporary };
} -> echo(out)
get /fail { const local = "secret"; throw new Error("inline failure"); } -> echo("must not run")
get /syntax { return invalid syntax; }
get /async { return Promise.resolve(1); }
get /empty { const local = "private"; }
get /probe { return [typeof local, typeof temporary, typeof before, typeof out]; }
"#,
    )]);
    for i in 1..=2 {
        let response = server.request("GET", "/item/mine").await;
        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&response.body).unwrap(),
            serde_json::json!({"id":"mine","call":i})
        );
    }
    for path in ["/fail", "/syntax"] {
        assert_eq!(
            server.request("GET", path).await.status,
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    assert_eq!(
        server.request("GET", "/async").await.status,
        StatusCode::NOT_IMPLEMENTED
    );
    assert_eq!(server.request("GET", "/empty").await.body, "null");
    assert_eq!(
        server.request("GET", "/probe").await.body,
        r#"["undefined","undefined","undefined","undefined"]"#
    );
    let source = fs::read_to_string(server.project.0.join("index.wx")).unwrap();
    server.project.write(
        "index.wx",
        &source.replace("const local = before;", "const local = before + ' edited';"),
    );
    let reloaded = server.request("GET", "/item/mine").await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&reloaded.body).unwrap(),
        serde_json::json!({"id":"mine edited","call":3})
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn put_form_bodies_decode_validate_and_remain_request_local() {
    let server = TestServer::start(&[(
        "index.wx",
        r#"
global { let calls = 0; function echo(value) { return value; } }
put /form/(id: String) form(title: String, completed: Boolean) -> echo(title): before {
    calls++;
    return { id, title, before, completed };
}
get /probe { return { calls, title: typeof title, completed: typeof completed }; }
post /form form(title: String) { return title; }
patch /form form(title: String) { return title; }
put /collision/(id: String) form(id: String) { return id; }
put /unsupported form(count: Number) { return count; }
"#,
    )]);
    let response = request_typed(
        server.addr,
        "PUT",
        "/form/1",
        "title=hello+%26+%2B+%2520+%F0%9F%8C%8D&completed=on&ignored=x",
        "application/x-www-form-urlencoded; charset=UTF-8",
    )
    .await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&response.body).unwrap(),
        serde_json::json!({"id":"1","title":"hello & + %20 🌍","before":"hello & + %20 🌍","completed":true})
    );
    for (body, media, expected) in [
        ("title=ok", "text/plain", 415),
        ("title=ok", "multipart/form-data; boundary=test", 415),
        (
            "title=ok",
            "application/x-www-form-urlencoded; charset=latin1",
            415,
        ),
        ("title=ok", "", 415),
        ("", "application/x-www-form-urlencoded", 400),
        ("title=%GG", "application/x-www-form-urlencoded", 400),
        ("title=%FF", "application/x-www-form-urlencoded", 400),
        ("title=a&title=b", "application/x-www-form-urlencoded", 400),
        (
            "title=a&completed=maybe",
            "application/x-www-form-urlencoded",
            400,
        ),
    ] {
        assert_eq!(
            request_typed(server.addr, "PUT", "/form/1", body, media)
                .await
                .status
                .as_u16(),
            expected
        );
    }
    for method in ["POST", "PATCH"] {
        let response = request_typed(
            server.addr,
            method,
            "/form",
            "title=body+value",
            "application/x-www-form-urlencoded",
        )
        .await;
        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(response.body, "body value");
    }
    assert_eq!(
        request_typed(
            server.addr,
            "PUT",
            "/collision/path",
            "id=body",
            "application/x-www-form-urlencoded"
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request_typed(
            server.addr,
            "PUT",
            "/unsupported",
            "count=3",
            "application/x-www-form-urlencoded"
        )
        .await
        .status,
        StatusCode::NOT_IMPLEMENTED
    );
    let large = format!("title={}", "x".repeat(65536));
    assert_eq!(
        request_typed(
            server.addr,
            "PUT",
            "/form/1",
            &large,
            "application/x-www-form-urlencoded"
        )
        .await
        .status,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let mut jobs = tokio::task::JoinSet::new();
    let barrier = Arc::new(tokio::sync::Barrier::new(17));
    for i in 0..16 {
        let addr = server.addr;
        let barrier = barrier.clone();
        jobs.spawn(async move {
            barrier.wait().await;
            let response = request_typed(addr, "PUT", &format!("/form/{i}"), &format!("title=client{i}"), "application/x-www-form-urlencoded").await;
            assert_eq!(response.status, StatusCode::OK);
            assert_eq!(serde_json::from_str::<serde_json::Value>(&response.body).unwrap(), serde_json::json!({"id":i.to_string(),"title":format!("client{i}"),"before":format!("client{i}"),"completed":false}));
        });
    }
    barrier.wait().await;
    while let Some(result) = jobs.join_next().await {
        result.unwrap();
    }
    let probe = server.request("GET", "/probe").await;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&probe.body).unwrap(),
        serde_json::json!({"calls":17,"title":"undefined","completed":"undefined"})
    );
}

#[tokio::test]
async fn chunked_form_upload_does_not_block_other_requests() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let server = TestServer::start(&[(
        "index.wx",
        "put /form form(title: String) { return title; }\nget /ready (ready)\n",
    )]);
    let mut stream = tokio::net::TcpStream::connect(server.addr).await.unwrap();
    stream.write_all(b"PUT /form HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/x-www-form-urlencoded\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\ntitle\r\n").await.unwrap();
    assert_eq!(server.request("GET", "/ready").await.body, "ready");
    stream.write_all(b"3\r\n=ok\r\n0\r\n\r\n").await.unwrap();
    let mut response = String::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_string(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.ends_with("ok"), "{response}");
}
