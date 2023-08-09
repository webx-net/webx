use std::path::PathBuf;

/// # WebX file format
/// A module for working with WebX files.
#[derive(Debug)]
pub struct WebXFile {
    /// The path to the file.
    pub path: PathBuf,
    /// The dependencies of the file.
    pub includes: Vec<String>,
    /// Scopes defined in the file.
    /// Created by root and the `location` keyword.
    pub scopes: Vec<WebXScope>,
}

#[derive(Debug)]
pub struct WebXScope {
    /// Global TypeScript code block
    pub global_ts: String,
    /// ORM Model definitions
    pub models: Vec<WebXModel>,
    /// Handler functions
    pub handlers: Vec<WebXHandler>,
    /// Route endpoints
    pub routes: Vec<WebXRoute>,
}

#[derive(Debug)]
pub struct WebXModel {
    /// The name of the model.
    pub name: String,
    /// The fields of the model.
    pub fields: String,
}

#[derive(Debug)]
pub struct WebXHandler {
    /// The name of the handler.
    pub name: String,
    /// The parameters of the handler.
    pub params: String,
    /// Return type of the handler.
    pub return_type: Option<String>,
    /// The typescript body of the handler.
    pub body: String,
}

#[derive(Debug)]
pub enum WebXRouteMethod {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
    OPTIONS,
    HEAD,
    CONNECT,
    TRACE,
    ANY,
}

pub fn route_from_str(method: String) -> Result<WebXRouteMethod, String> {
    match method.to_uppercase().as_str() {
        "GET" => Ok(WebXRouteMethod::GET),
        "POST" => Ok(WebXRouteMethod::POST),
        "PUT" => Ok(WebXRouteMethod::PUT),
        "PATCH" => Ok(WebXRouteMethod::PATCH),
        "DELETE" => Ok(WebXRouteMethod::DELETE),
        "OPTIONS" => Ok(WebXRouteMethod::OPTIONS),
        "HEAD" => Ok(WebXRouteMethod::HEAD),
        "CONNECT" => Ok(WebXRouteMethod::CONNECT),
        "TRACE" => Ok(WebXRouteMethod::TRACE),
        "ANY" => Ok(WebXRouteMethod::ANY),
        _ => Err(format!("Invalid route method: {}", method)),
    }
}

#[derive(Debug)]
pub struct WebXRoute {
    /// HTTP method of the route.
    pub method: WebXRouteMethod,
    /// The path of the route.
    pub path: String,
    /// The pre-handler functions of the route.
    pub pre_handlers: Vec<String>,
    /// The code block of the route.
    pub code: Option<String>,
    /// The post-handler functions of the route.
    pub post_handlers: Vec<String>,
}
