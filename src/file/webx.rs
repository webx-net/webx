use std::path::PathBuf;

pub type WXType = String;

pub type WXTypedIdentifier = (String, WXType);

#[derive(Debug)]
pub enum WXUrlPathSegment {
    Literal(String),
    Parameter(WXTypedIdentifier),
    Regex(String),
}

pub type WXUrlPath = Vec<WXUrlPathSegment>;
pub const WXRootPath: WXUrlPath = WXUrlPath::new();

/// # WebX file format
/// A module for working with WebX files.
#[derive(Debug)]
pub struct WXFile {
    /// The path to the file.
    pub path: PathBuf,
    /// Global webx module scope.
    pub module_scope: WXScope,
}

impl WXFile {
    /// "/path/to/file.webx" -> "path/to"
    pub fn parent(&self) -> String {
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
        let path = self.path.canonicalize().unwrap();
        let stripped = path
            .strip_prefix(&cwd)
            .expect(&format!("Failed to strip prefix of {:?}", path));
        stripped
            .parent()
            .unwrap()
            .to_str()
            .unwrap()
            .replace("\\", "/")
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
pub struct WXScope {
    pub path: WXUrlPath,
    /// The dependencies of the scope.
    pub includes: Vec<String>,
    /// Global TypeScript code block
    pub global_ts: String,
    /// ORM Model definitions
    pub models: Vec<WXModel>,
    /// Handler functions
    pub handlers: Vec<WXHandler>,
    /// Route endpoints
    pub routes: Vec<WXRoute>,
    /// Nested scopes.
    /// Created by root and the `location` keyword.
    pub scopes: Vec<WXScope>,
}

#[derive(Debug)]
pub struct WXModel {
    /// The name of the model.
    pub name: String,
    /// The fields of the model.
    pub fields: Vec<WXTypedIdentifier>,
}

#[derive(Debug)]
pub struct WXHandler {
    /// The name of the handler.
    pub name: String,
    /// The parameters of the handler.
    pub params: Vec<WXTypedIdentifier>,
    /// The typescript body of the handler.
    pub body: WXBody,
}

#[derive(Debug)]
pub enum WXRouteMethod {
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
pub enum WXBodyType {
    TS,
    TSX,
    JSON,
    TEXT,
}

#[derive(Debug)]
pub struct WXBody {
    pub body_type: WXBodyType,
    pub body: String,
}

#[derive(Debug)]
pub struct WXRoute {
    /// HTTP method of the route.
    pub method: WXRouteMethod,
    /// The path of the route.
    pub path: WXUrlPath,
    /// Request body format.
    pub body_format: Option<String>,
    /// The pre-handler functions of the route.
    pub pre_handlers: Vec<String>,
    /// The code block of the route.
    pub body: Option<WXBody>,
    /// The post-handler functions of the route.
    pub post_handlers: Vec<String>,
}
