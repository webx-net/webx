use std::{
    fmt::{self, Formatter},
    path::PathBuf,
};

pub type WXType = String;

#[derive(Clone)]
pub struct WXTypedIdentifier {
    pub name: String,
    pub type_: WXType,
}

impl fmt::Debug for WXTypedIdentifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.type_)
    }
}

#[derive(Debug, Clone)]
pub enum WXUrlPathSegment {
    Literal(String),
    Parameter(WXTypedIdentifier),
    Regex(String),
}

pub struct WXUrlPath(pub Vec<WXUrlPathSegment>);

impl fmt::Debug for WXUrlPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let c = self.0.clone();
        let ss = c
            .into_iter()
            .map(|segment| match segment {
                WXUrlPathSegment::Literal(literal) => literal,
                WXUrlPathSegment::Parameter(WXTypedIdentifier { name, type_ }) => {
                    format!("({}: {})", name, type_)
                }
                WXUrlPathSegment::Regex(regex) => format!("({})", regex),
            })
            .collect::<Vec<_>>();
        write!(f, "/")?;
        for (i, s) in ss.iter().enumerate() {
            if i > 0 {
                write!(f, "/")?;
            }
            write!(f, "{}", s)?;
        }
        Ok(())
    }
}

pub const WXROOT_PATH: WXUrlPath = WXUrlPath(vec![]);

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

pub enum WXBodyType {
    TS,
    TSX,
    // TODO: JSON and TEXT
}

impl ToString for WXBodyType {
    fn to_string(&self) -> String {
        match self {
            WXBodyType::TS => "ts".to_string(),
            WXBodyType::TSX => "tsx".to_string(),
        }
    }
}

pub struct WXBody {
    pub body_type: WXBodyType,
    pub body: String,
}

impl fmt::Debug for WXBody {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "```{}\n{}\n```", self.body_type.to_string(), self.body)
    }
}

pub enum WXRouteReqBody {
    ModelReference(String),
    Definition(String, Vec<WXTypedIdentifier>),
}

impl fmt::Debug for WXRouteReqBody {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            WXRouteReqBody::ModelReference(name) => write!(f, "{}", name),
            WXRouteReqBody::Definition(name, fields) => {
                write!(f, "{}(", name)?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    field.fmt(f)?;
                }
                write!(f, ")")
            }
        }
    }
}

pub struct WXRouteHandler {
    pub name: String,
    pub args: Vec<String>,
    pub output: Option<String>,
}

impl fmt::Debug for WXRouteHandler {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.name, self.args.join(", "))?;
        if let Some(output) = &self.output {
            write!(f, " : {}", output)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct WXRoute {
    /// HTTP method of the route.
    pub method: WXRouteMethod,
    /// The path of the route.
    pub path: WXUrlPath,
    /// Request body format.
    pub body_format: Option<WXRouteReqBody>,
    /// The pre-handler functions of the route.
    pub pre_handlers: Vec<WXRouteHandler>,
    /// The code block of the route.
    pub body: Option<WXBody>,
    /// The post-handler functions of the route.
    pub post_handlers: Vec<WXRouteHandler>,
}
