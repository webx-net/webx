pub mod requests {

    use hyper::body::Incoming;

    pub fn serialize(request: &hyper::Request<Incoming>) -> String {
        let mut result = format!(
            "{} {} {:?}\r\n",
            request.method(),
            request.uri(),
            request.version()
        );
        for (header, value) in request.headers() {
            result.push_str(&format!("{}: {}\r\n", header, value.to_str().unwrap_or("")));
        }
        if matches!(*request.method(), hyper::Method::POST | hyper::Method::PUT) {
            result.push_str("\r\n");
            result.push_str(&format!("{:?}", request.body()));
        }
        result
    }
}

pub mod responses {
    use hyper::Response;

    use crate::runner::WXMode;

    pub fn serialize<D: Default + ToString>(response: &Response<D>) -> String {
        let mut result = format!("HTTP/1.1 {}\r\n", response.status());
        for (header, value) in response.headers() {
            result.push_str(&format!("{}: {}\r\n", header, value.to_str().unwrap_or("")));
        }
        result.push_str("\r\n");
        result.push_str(&response.body().to_string());
        result
    }

    pub fn ok_html(body: String) -> Response<String> {
        Response::builder()
            .status(hyper::StatusCode::OK)
            .header("Access-Control-Allow-Origin", "*")
            .header("Content-Type", "text/html; charset=utf-8")
            .header("Content-Length", body.len().to_string())
            .header("Connection", "close")
            .header("Server", "webx")
            .header("Date", chrono::Utc::now().to_rfc2822())
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache")
            .header("Expires", "0")
            .body(body)
            .unwrap()
    }

    pub fn not_found_default_webx(mode: WXMode) -> Response<String> {
        let body = if mode.is_dev() {
            format!(
                r#"<html>
    <head>
        <title>404 Not Found</title>
    </head>
    <body>
        <h1>404 Not Found</h1>
        <p>The requested URL was not found on this server.</p>
        <hr>
        <address>webx/0.1.0 (Unix) (webx/{})</address>
    </body>
</html>"#,
                env!("CARGO_PKG_VERSION")
            )
        } else {
            r#"<html>
    <head>
        <title>404 Not Found</title>
    </head>
    <body>
        <h1>404 Not Found</h1>
        <p>The requested URL was not found on this server.</p>
        <hr>
        <address>webx/0.1.0 (Unix)</address>
    </body>
</html>"#
                .to_string()
        };
        Response::builder()
            .status(hyper::StatusCode::NOT_FOUND)
            .header("Access-Control-Allow-Origin", "*")
            .header("Content-Type", "text/html; charset=utf-8")
            .header("Content-Length", body.len().to_string())
            .header("Connection", "close")
            .header("Server", "webx")
            .header("Date", chrono::Utc::now().to_rfc2822())
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache")
            .header("Expires", "0")
            .body(body)
            .unwrap()
    }

    pub fn internal_server_error_default_webx(mode: WXMode, message: String) -> Response<String> {
        let body = if mode.is_dev() {
            format!(
                r#"<html>
    <head>
        <title>500 Internal Server Error</title>
    </head>
    <body>
        <h1>500 Internal Server Error</h1>
        <p>
            The server encountered an internal error and was unable to complete your request. <br>
            Either the server is overloaded or there is an error in the application.
        </p>
        <h2>Debugging Information</h2>
        <p>
            <strong>Message:</strong>
            <pre>
{}
            </pre>
        </p>
        <hr>
        <address>webx/{} development mode</address>
    </body>
</html>"#,
                message,
                env!("CARGO_PKG_VERSION")
            )
        } else {
            r#"<html>
    <head>
        <title>500 Internal Server Error</title>
    </head>
    <body>
        <h1>500 Internal Server Error</h1>
        <p>
            The server encountered an internal error and was unable to complete your request. <br>
            Either the server is overloaded or there is an error in the application.
        </p>
        <hr>
        <address>webx prouction mode</address>
    </body>
</html>"#
                .to_string()
        };
        Response::builder()
            .status(hyper::StatusCode::INTERNAL_SERVER_ERROR)
            .header("Access-Control-Allow-Origin", "*")
            .header("Content-Type", "text/html; charset=utf-8")
            .header("Content-Length", body.len().to_string())
            .header("Connection", "close")
            .header("Server", "webx")
            .header("Date", chrono::Utc::now().to_rfc2822())
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache")
            .header("Expires", "0")
            .body(body)
            .unwrap()
    }
}
