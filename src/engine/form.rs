//! URL-encoded form bindings for declared request bodies.
use super::runtime::WXRuntimeError;
use crate::file::webx::WXRouteReqBody;
use hyper::{body::Bytes, Request};
use serde_json::Value;
use std::collections::HashMap;

fn error(code: i32, message: impl Into<String>) -> WXRuntimeError {
    WXRuntimeError {
        code,
        message: message.into(),
    }
}

fn component(bytes: &[u8]) -> Result<String, WXRuntimeError> {
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => decoded.push(b' '),
            b'%' => {
                let hex = |b: u8| (b as char).to_digit(16).map(|n| n as u8);
                let pair = bytes
                    .get(i + 1)
                    .and_then(|b| hex(*b))
                    .zip(bytes.get(i + 2).and_then(|b| hex(*b)));
                let (a, b) =
                    pair.ok_or_else(|| error(400, "Invalid percent escape in form body"))?;
                decoded.push(a * 16 + b);
                i += 2;
            }
            b => decoded.push(b),
        }
        i += 1;
    }
    String::from_utf8(decoded).map_err(|_| error(400, "Form body must be UTF-8"))
}

pub fn bindings(
    req: &Request<Bytes>,
    format: Option<&WXRouteReqBody>,
) -> Result<HashMap<String, Value>, WXRuntimeError> {
    let Some(format) = format else {
        return Ok(HashMap::new());
    };
    let WXRouteReqBody::Definition(name, fields) = format else {
        return Err(error(501, "Model body decoding is not supported"));
    };
    if name != "form" {
        return Err(error(501, "Only form request body decoding is supported"));
    }
    let content_type = req
        .headers()
        .get(hyper::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let mut parts = content_type.split(';').map(str::trim);
    if !parts
        .next()
        .unwrap_or("")
        .eq_ignore_ascii_case("application/x-www-form-urlencoded")
    {
        return Err(error(415, "Expected application/x-www-form-urlencoded"));
    }
    for parameter in parts {
        if let Some((key, value)) = parameter.split_once('=') {
            if key.trim().eq_ignore_ascii_case("charset")
                && !value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8")
            {
                return Err(error(415, "Only UTF-8 form bodies are supported"));
            }
        }
    }
    let mut inputs = HashMap::new();
    for pair in req.body().split(|b| *b == b'&').filter(|p| !p.is_empty()) {
        let mut parts = pair.splitn(2, |b| *b == b'=');
        let key = component(parts.next().unwrap())?;
        let value = component(parts.next().unwrap_or_default())?;
        if inputs.insert(key, value).is_some() {
            return Err(error(400, "Duplicate form field"));
        }
    }
    let mut values = HashMap::new();
    for field in fields {
        let input = inputs.get(&field.name);
        let value = match field.type_.to_ascii_lowercase().as_str() {
            "string" => Value::String(
                input
                    .ok_or_else(|| error(400, format!("Missing form field: {}", field.name)))?
                    .clone(),
            ),
            "bool" | "boolean" => Value::Bool(match input.map(String::as_str) {
                None | Some("false" | "off" | "0") => false,
                Some("true" | "on" | "1") => true,
                _ => {
                    return Err(error(
                        400,
                        format!("Invalid boolean form field: {}", field.name),
                    ))
                }
            }),
            _ => {
                return Err(error(
                    501,
                    format!("Unsupported form field type: {}", field.type_),
                ))
            }
        };
        if values.insert(field.name.clone(), value).is_some() {
            return Err(error(500, "Duplicate form field declaration"));
        }
    }
    Ok(values)
}
