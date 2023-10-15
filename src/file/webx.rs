use std::{
    fmt::{self, Formatter, Display, Debug},
    path::PathBuf, hash::{Hash, Hasher},
};

use http::Uri;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct WXInfoField {
    pub path: WXModulePath,
    pub line: usize,
}

pub type WXType = String;

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct WXTypedIdentifier {
    pub name: String,
    pub type_: WXType,
}

impl fmt::Debug for WXTypedIdentifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.type_)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WXUrlPathSegment {
    Literal(String),
    Parameter(WXTypedIdentifier),
    Regex(String, String), // Name, Regex
}

#[derive(PartialEq, Eq, Clone)]
pub struct WXUrlPath(pub Vec<WXUrlPathSegment>);

impl Display for WXUrlPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let c = self.0.clone();
        let ss = c
            .into_iter()
            .map(|segment| match segment {
                WXUrlPathSegment::Literal(literal) => literal,
                WXUrlPathSegment::Parameter(WXTypedIdentifier { name, type_ }) => {
                    format!("({}: {})", name, type_)
                }
                WXUrlPathSegment::Regex(_, regex) => format!("({})", regex),
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

impl Debug for WXUrlPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Hash for WXUrlPath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for segment in self.0.iter() {
            match segment {
                WXUrlPathSegment::Literal(literal) => literal.hash(state),
                WXUrlPathSegment::Parameter(WXTypedIdentifier { name, type_ }) => {
                    name.hash(state);
                    type_.hash(state);
                }
                WXUrlPathSegment::Regex(regex_name, regex) => format!("{}{}", regex_name, regex).hash(state),
            }
        }
    }
}

pub type WXPathBindings = Vec<(String, String)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WXPathResolution {
    None,
    Perfect(WXPathBindings),
    Partial(WXPathBindings),
}

impl WXUrlPath {
    pub fn combine(&self, other: &WXUrlPath) -> WXUrlPath {
        let mut path = self.0.clone();
        path.extend(other.0.clone());
        WXUrlPath(path)
    }

    pub fn segments(&self) -> usize {
        self.0.len()
    }

    pub fn matches(&self, url: &Uri) -> WXPathResolution {
        let mut url = url.path().split('/').skip(1);
        let url_count = url.clone().count();
        dbg!(url.clone().collect::<Vec<_>>(), url_count, self.segments());
        let mut bindings = WXPathBindings::new();
        if self.segments() == url_count {
            for (pattern, part) in self.0.iter().zip(url) {
                match pattern {
                    WXUrlPathSegment::Literal(literal) => {
                        if literal.as_str() != part { return WXPathResolution::None; }
                    }
                    WXUrlPathSegment::Parameter(WXTypedIdentifier { name, type_ }) => {
                        // TODO: Check type.
                        bindings.push((name.clone(), part.to_string()));
                    }
                    WXUrlPathSegment::Regex(regex_name, regex) => {
                        if regex::Regex::new(regex).unwrap().is_match(part) {
                            bindings.push((regex_name.clone(), part.to_string()));
                        } else {
                            return WXPathResolution::None;
                        }
                    }
                }
            }
            return WXPathResolution::Perfect(bindings);
        } else {
            if self.segments() < url_count { return WXPathResolution::None; }
            // self.segments() > url_count
            // This could be a partial match in case some regex is allowed to be empty.
            for segment in self.0.iter() {
                match segment {
                    WXUrlPathSegment::Literal(literal) => {
                        if let Some(part) = url.next() {
                            if part != literal.as_str() {
                                return WXPathResolution::None;
                            }
                        } else {
                            return WXPathResolution::None;
                        }
                    }
                    WXUrlPathSegment::Parameter(WXTypedIdentifier { name, type_ }) => {
                        if let Some(part) = url.next() {
                            // TODO: Check type.
                            bindings.push((name.clone(), part.to_string()));
                        } else {
                            return WXPathResolution::None;
                        }
                    }
                    WXUrlPathSegment::Regex(regex_name, regex) => {
                        if let Some(part) = url.next() {
                            if regex::Regex::new(regex).unwrap().is_match(part) {
                                bindings.push((regex_name.clone(), part.to_string()));
                            } else {
                                return WXPathResolution::None;
                            }
                        } else {
                            if regex::Regex::new(regex).unwrap().is_match("") {
                                bindings.push((regex_name.clone(), "".to_string()));
                            } else {
                                return WXPathResolution::None;
                            }
                        }
                    }
                }
            }
            // Check if there are any remaining segments in the url.
            if let Some(_) = url.next() {
                return WXPathResolution::None;
            }
            return WXPathResolution::Partial(bindings);
        }
    }
}

pub const WXROOT_PATH: WXUrlPath = WXUrlPath(vec![]);

/// # WebX module
/// A file data structure for WebX files.
#[derive(Debug)]
pub struct WXModule {
    /// The path to the file.
    pub path: WXModulePath,
    /// Global webx module scope.
    pub scope: WXScope,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct  WXModulePath {
    pub inner: PathBuf,
}

impl WXModulePath {
    pub fn new(inner: PathBuf) -> Self {
        Self { inner }
    }
    /// "/path/to/file.webx" -> "path/to"
    pub fn parent(&self) -> String {
        let cwd = std::env::current_dir().unwrap().canonicalize().unwrap();
        let path = self.inner.canonicalize().unwrap();
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
        match self.inner.file_name() {
            Some(name) => match name.to_str() {
                Some(name) => match name.split('.').next() {
                    Some(name) => name,
                    None => panic!("Failed to extract file module name of {:?}", self.inner),
                },
                None => panic!("Failed to convert file name to string of {:?}", self.inner),
            },
            None => panic!("Failed to get file name of {:?}", self.inner),
        }
    }

    /// "/path/to/file.webx" -> "path/to/file"
    pub fn module_name(&self) -> String {
        format!("{}/{}", self.parent(), self.name())
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct WXModel {
    /// The name of the model.
    pub name: String,
    /// The fields of the model.
    pub fields: Vec<WXTypedIdentifier>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct WXHandler {
    /// The name of the handler.
    pub name: String,
    /// The parameters of the handler.
    pub params: Vec<WXTypedIdentifier>,
    /// The typescript body of the handler.
    pub body: WXBody,
}

#[derive(Hash, PartialEq, Eq, Clone)]
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

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct WXBody {
    pub body_type: WXBodyType,
    pub body: String,
}

impl fmt::Debug for WXBody {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "```{}\n{}\n```", self.body_type.to_string(), self.body)
    }
}

#[derive(Hash, PartialEq, Eq, Clone)]
pub enum WXRouteReqBody {
    ModelReference(String),
    Definition(String, Vec<WXTypedIdentifier>),
}

impl Display for WXRouteReqBody {
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

impl Debug for WXRouteReqBody {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Hash, PartialEq, Eq, Clone)]
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

#[derive(Debug, Clone)]
pub struct WXRoute {
    pub info: WXInfoField,
    /// HTTP method of the route.
    pub method: http::Method,
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

impl Hash for WXRoute {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.method.hash(state);
        self.path.hash(state);
    }
}

impl PartialEq for WXRoute {
    fn eq(&self, other: &Self) -> bool {
        self.method == other.method && self.path == other.path
    }
}

impl Eq for WXRoute {}