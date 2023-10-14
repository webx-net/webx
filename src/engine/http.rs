use std::{io::{BufReader, BufRead, Read}, net::TcpStream};

use http::{Request, Version, request::Builder, Response};

pub fn parse_request_tcp<D: Default>(stream: &TcpStream) -> Option<Request<D>> {
    let reader = BufReader::new(stream);
    parse_request(reader)
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
    let mut result = format!("HTTP/1.1 {} {}\r\n", response.status(), response.status().canonical_reason().unwrap_or("Unknown"));
    for (header, value) in response.headers() {
        result.push_str(&format!("{}: {}\r\n", header, value.to_str().unwrap_or("")));
    }
    result.push_str("\r\n");
    result.push_str(&response.body().to_string());
    result
}