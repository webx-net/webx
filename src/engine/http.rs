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

    pub fn server_header(mode: WXMode) -> String {
        if mode.is_dev() {
            format!("webx/{}", env!("CARGO_PKG_VERSION"))
        } else {
            "webx".to_string()
        }
    }

    pub fn server_banner(mode: WXMode) -> String {
        if mode.is_dev() {
            format!("{} development mode", server_header(mode))
        } else {
            "webx".to_string()
        }
    }

    pub fn serialize<D: Default + ToString>(response: &Response<D>) -> String {
        let mut result = format!("HTTP/1.1 {}\r\n", response.status());
        for (header, value) in response.headers() {
            result.push_str(&format!("{}: {}\r\n", header, value.to_str().unwrap_or("")));
        }
        result.push_str("\r\n");
        result.push_str(&response.body().to_string());
        result
    }

    pub fn ok_html<T>(body: T, len: usize, mode: WXMode) -> Response<T> {
        Response::builder()
            .status(hyper::StatusCode::OK)
            .header("Access-Control-Allow-Origin", "*")
            .header("Content-Type", "text/html; charset=utf-8")
            .header("Content-Length", len.to_string())
            .header("Connection", "close")
            .header("Server", server_header(mode))
            .header("Date", chrono::Utc::now().to_rfc2822())
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache")
            .header("Expires", "0")
            .body(body)
            .unwrap()
    }

    pub fn not_found_default_webx(mode: WXMode) -> Response<String> {
        let body = format!(
            r#"<html>
    <head>
        <title>404 Not Found</title>
    </head>
    <body>
        <h1>404 Not Found</h1>
        <p>The requested URL was not found on this server.</p>
        <hr>
        <address>{}</address>
    </body>
</html>"#,
            server_banner(mode)
        );
        Response::builder()
            .status(hyper::StatusCode::NOT_FOUND)
            .header("Access-Control-Allow-Origin", "*")
            .header("Content-Type", "text/html; charset=utf-8")
            .header("Content-Length", body.len().to_string())
            .header("Connection", "close")
            .header("Server", server_header(mode))
            .header("Date", chrono::Utc::now().to_rfc2822())
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache")
            .header("Expires", "0")
            .body(body)
            .unwrap()
    }

    pub fn internal_server_error_default_webx(mode: WXMode, message: String) -> Response<String> {
        let body = format!(
            r#"<html>
    <head>
        <title>500 Internal Server Error</title>
    </head>
    <body>
        <h1>500 Internal Server Error</h1>
        <p>
            The server encountered an internal error and was unable to complete your request. <br>
            Either the server is overloaded or there is an error in the application.
        </p>{}
        <hr>
        <address>{}</address>
    </body>
</html>"#,
            if message.is_empty() {
                ""
            } else {
                &format!(
                    r#"
        <h2>Debugging Information</h2>
        <p>
            <strong>Message:</strong>
            <pre>
{}
            </pre>
        </p>"#,
                    message
                )
            },
            server_banner(mode)
        );
        Response::builder()
            .status(hyper::StatusCode::INTERNAL_SERVER_ERROR)
            .header("Access-Control-Allow-Origin", "*")
            .header("Content-Type", "text/html; charset=utf-8")
            .header("Content-Length", body.len().to_string())
            .header("Connection", "close")
            .header("Server", server_header(mode))
            .header("Date", chrono::Utc::now().to_rfc2822())
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache")
            .header("Expires", "0")
            .body(body)
            .unwrap()
    }
}
