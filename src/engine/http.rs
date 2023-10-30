use std::{io::{BufReader, BufRead, Read}, net::TcpStream};

use http::{Request, Version, request::Builder, Response};

pub fn read_all_from_stream(stream: &TcpStream) -> String {
    let mut reader = BufReader::new(stream);
    let mut result = String::new();
    reader.read_to_string(&mut result).unwrap_or(0);
    result
}

pub fn parse_request_from_string<D: Default>(request: &str) -> Option<Request<D>> {
    parse_request(BufReader::new(request.as_bytes()))
}

pub fn parse_request<D: Default, T: Read>(reader: BufReader<T>) -> Option<Request<D>> {
    let mut lines = reader.lines().map(|l| l.unwrap_or("".into())).take_while(|l| !l.is_empty());
    let r = Request::builder();
    let r = parse_request_line(lines.next()?, r)?;
    let r = parse_request_headers(lines, r)?;
    // let request = parse_http_request_body(reader)?;
    let r = r.body(D::default()).unwrap();
    Some(r)
}

fn parse_request_line(line: String, request: Builder) -> Option<Builder> {
    let mut reqline = line.split_whitespace();
    let method = reqline.next()?;
    let path = reqline.next()?;
    // Refrence:
    // - https://developer.mozilla.org/en-US/docs/Web/HTTP/Messages
    // - https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/Evolution_of_HTTP
    let version = match reqline.next()? {
        "HTTP/1.0" => Version::HTTP_10,
        "HTTP/1.1" => Version::HTTP_11,
        "HTTP/2.0" => Version::HTTP_2,
        "HTTP/3.0" => Version::HTTP_3,
        _ => Version::HTTP_09, // No version
    };
    Some(request.method(method).uri(path).version(version))
}

fn parse_request_headers(lines: impl Iterator<Item=String>, request: Builder) -> Option<Builder> {
    let mut request = request;
    for line in lines {
        let mut header = line.splitn(2, ':');
        request = request.header(
            header.next()?.trim(),
            header.next()?.trim()
        );
    }
    Some(request)
}

pub fn serialize_response<D: Default + ToString>(response: &Response<D>) -> String {
    let mut result = format!("HTTP/1.1 {}\r\n", response.status());
    for (header, value) in response.headers() {
        result.push_str(&format!("{}: {}\r\n", header, value.to_str().unwrap_or("")));
    }
    result.push_str("\r\n");
    result.push_str(&response.body().to_string());
    result
}

pub mod responses {
    use crate::runner::WXMode;

    pub fn ok_html(body: String) -> http::Response<String> {
        http::Response::builder()
            .status(http::StatusCode::OK)
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

    pub fn not_found_default_webx(mode: WXMode) -> http::Response<String> {
        let body = if mode.is_dev() { format!(r#"
                <html>
                    <head>
                        <title>404 Not Found</title>
                    </head>
                    <body>
                        <h1>404 Not Found</h1>
                        <p>The requested URL was not found on this server.</p>
                        <hr>
                        <address>webx/0.1.0 (Unix) (webx/{})</address>
                    </body>
                </html>
            "#, env!("CARGO_PKG_VERSION")) } else { format!(r#"
                <html>
                    <head>
                        <title>404 Not Found</title>
                    </head>
                    <body>
                        <h1>404 Not Found</h1>
                        <p>The requested URL was not found on this server.</p>
                        <hr>
                        <address>webx/0.1.0 (Unix)</address>
                    </body>
                </html>
            "#) };
        http::Response::builder()
            .status(http::StatusCode::NOT_FOUND)
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

    pub fn internal_server_error_default_webx(mode: WXMode, message: String) -> http::Response<String> {
        let body = if mode.is_dev() { format!(r#"
                <html>
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
                </html>
            "#, message, env!("CARGO_PKG_VERSION")) } else { format!(r#"
                <html>
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
                </html>
            "#) };
        http::Response::builder()
            .status(http::StatusCode::INTERNAL_SERVER_ERROR)
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
