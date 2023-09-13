use std::path::PathBuf;

/// # WebX file format
/// A module for working with WebX files.
#[derive(Debug)]
pub struct WebXFile {
    /// The path to the file.
    pub path: PathBuf,
    /// Global webx module scope.
    pub module_scope: WebXScope,
}

impl WebXFile {
    /// "/path/to/file.webx" -> "path/to"
    pub fn parent(&self) -> String {
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
        let path = self.path.canonicalize().unwrap();
        let stripped = path.strip_prefix(&cwd).expect(&format!("Failed to strip prefix of {:?}", path));
        stripped.parent().unwrap().to_str().unwrap().replace("\\", "/")
    }

    /// "/path/to/file.webx" -> "file"
    pub fn name(&self) -> &str {
        match self.path.file_name() {
            Some(name) => match name.to_str() {
                Some(name) => match name.split('.').next() {
                    Some(name) => name,
                    None => panic!("Failed to extract file module name of {:?}", self.path),
                },
                None => panic!("Failed to convert file name to string of {:?}", self.path),
            },
            None => panic!("Failed to get file name of {:?}", self.path),
        }
    }

    /// "/path/to/file.webx" -> "path/to/file"
    pub fn module_name(&self) -> String {
        format!("{}/{}", self.parent(), self.name())
    }
}

#[derive(Debug)]
pub struct WebXScope {
    /// The dependencies of the scope.
    pub includes: Vec<String>,
    /// Global TypeScript code block
    pub global_ts: String,
    /// ORM Model definitions
    pub models: Vec<WebXModel>,
    /// Handler functions
    pub handlers: Vec<WebXHandler>,
    /// Route endpoints
    pub routes: Vec<WebXRoute>,
    /// Nested scopes.
    /// Created by root and the `location` keyword.
    pub scopes: Vec<WebXScope>,
}

pub type TypedIdentifier = (String, String);

#[derive(Debug)]
pub struct WebXModel {
    /// The name of the model.
    pub name: String,
    /// The fields of the model.
    pub fields: Vec<TypedIdentifier>,
}

#[derive(Debug)]
pub struct WebXHandler {
    /// The name of the handler.
    pub name: String,
    /// The parameters of the handler.
    pub params: Vec<TypedIdentifier>,
    /// The typescript body of the handler.
    pub body: String,
}

#[derive(Debug)]
pub enum WebXRouteMethod {
    CONNECT,
    DELETE,
    GET,
    HEAD,
    OPTIONS,
    PATCH,
    POST,
    PUT,
    TRACE,
}

#[derive(Debug)]
pub struct WebXRoute {
    /// HTTP method of the route.
    pub method: WebXRouteMethod,
    /// The path of the route.
    pub path: String,
    /// Request body format.
    pub body_format: Option<String>,
    /// The pre-handler functions of the route.
    pub pre_handlers: Vec<String>,
    /// The code block of the route.
    pub body: Option<String>,
    /// The post-handler functions of the route.
    pub post_handlers: Vec<String>,
}
