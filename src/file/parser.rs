use crate::{
    file::webx::WXModule,
    reporting::error::{
        exit_error, exit_error_expected_any_of_but_found, exit_error_expected_but_found,
        exit_error_unexpected, exit_error_unexpected_char, ERROR_PARSE_IO, ERROR_SYNTAX,
    },
};
use std::{
    io::{BufReader, Read},
    path::PathBuf,
};

use super::webx::{
    WXBody, WXBodyType, WXHandler, WXModel, WXRoute, WXRouteHandler, WXRouteReqBody,
    WXScope, WXTypedIdentifier, WXUrlPath, WXUrlPathSegment, WXROOT_PATH, WXInfoField, WXModulePath,
};

struct WebXFileParser<'a> {
    file: &'a PathBuf,
    _content: &'a String,
    reader: BufReader<&'a [u8]>,
    line: usize,
    column: usize,
    peeked_index: u64, // "next index"
    next_index: u64,   // "current index"
    peeked: Option<char>,
}

impl<'a> WebXFileParser<'a> {
    fn new(file: &'a PathBuf, content: &'a String) -> WebXFileParser<'a> {
        let mut p = WebXFileParser {
            file,
            _content: content,
            reader: BufReader::new(content.as_bytes()),
            line: 1,
            column: 1,
            peeked_index: 0,
            next_index: 0,
            peeked: None,
        };
        p.peeked = p.__raw_next();
        p
    }

    fn __update_line_column(&mut self, c: char) {
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
    }

    /// Returns the next character in the file, or None if EOF is reached.
    /// Increments the line and column counters.
    ///
    /// # Errors
    /// If the file cannot be read, an error is returned and the program exits.
    fn __raw_next(&mut self) -> Option<char> {
        let mut buf = [0; 1];
        let bytes_read = match self.reader.read(&mut buf) {
            Ok(n) => n,
            Err(e) => exit_error(
                format!(
                    "Failed to read file '{}' due to, {}",
                    self.file.display(),
                    e
                ),
                ERROR_PARSE_IO,
            ),
        };
        if bytes_read == 0 {
            return None;
        }
        let c = buf[0] as char;
        self.peeked_index += 1;
        // Index of the character returned by the next call to `next`.
        self.next_index = self.peeked_index - 1;
        Some(c)
    }

    fn peek(&self) -> Option<char> {
        self.peeked
    }
    fn next(&mut self) -> Option<char> {
        let c = self.peeked;
        self.peeked = self.__raw_next();
        if let Some(c) = c {
            self.__update_line_column(c);
        }
        c
    }
    fn expect(&mut self, context: &str) -> char {
        let nc = self.next();
        self.expect_not_eof(nc, context)
    }

    fn expect_not_eof(&mut self, nc: Option<char>, context: &str) -> char {
        if nc.is_none() {
            exit_error_unexpected(
                "EOF".to_string(),
                context,
                self.line,
                self.column,
                ERROR_SYNTAX,
            );
        }
        nc.unwrap()
    }

    /// Expect a specific character to be next in the file.
    /// Increments the line and column counters.
    ///
    /// # Errors
    /// If EOF is reached, or the next character is not the expected one, an error is returned and the program exits.
    fn expect_specific(&mut self, nc: Option<char>, expected: char, context: &str) -> char {
        let nc = self.expect_not_eof(nc, context);
        if nc != expected {
            exit_error_expected_but_found(
                format!("'{}'", expected),
                nc.to_string(),
                context,
                self.line,
                self.column,
                ERROR_SYNTAX,
            );
        }
        nc
    }

    fn expect_next_specific(&mut self, expected: char, context: &str) -> char {
        let nc = self.next();
        self.expect_specific(nc, expected, context)
    }

    fn expect_specific_str(&mut self, expected: &str, already_read: usize, context: &str) {
        for c in expected.chars().skip(already_read) {
            if self.expect(context) != c {
                exit_error_expected_but_found(
                    expected.to_string(),
                    c.to_string(),
                    context,
                    self.line,
                    self.column,
                    ERROR_SYNTAX,
                );
            }
        }
    }

    /// Expect any of the given characters to be next in the file.
    /// Increments the line and column counters.
    ///
    /// # Errors
    /// If EOF is reached, or the next character is not one of the expected ones, an error is returned and the program exits.
    fn expect_any_of(&mut self, nc: Option<char>, cs: Vec<char>, context: &str) -> char {
        let nc = self.expect_not_eof(nc, context);
        if !cs.contains(&nc) {
            exit_error_expected_any_of_but_found(
                format!("{:?}", cs),
                nc,
                context,
                self.line,
                self.column,
                ERROR_SYNTAX,
            );
        }
        nc
    }

    fn expect_next_any_of(&mut self, cs: Vec<char>, context: &str) -> char {
        let nc = self.next();
        self.expect_any_of(nc, cs, context)
    }

    fn skip_whitespace(&mut self, skip_newlines: bool) {
        loop {
            let c = self.peek();
            if c.is_none() {
                break;
            }
            let c = c.unwrap();
            if c == ' ' || c == '\t' || (skip_newlines && c == '\n') {
                self.next();
            } else {
                break;
            }
        }
    }

    fn next_skip_whitespace(&mut self, skip_newlines: bool) -> Option<char> {
        loop {
            let c = self.next();
            if c.is_none() {
                break;
            }
            let c = c.unwrap();
            if c == ' ' || c == '\t' || (skip_newlines && c == '\n') {
                continue;
            }
            return Some(c); // Return the first non-whitespace character.
        }
        None
    }

    fn read_until_any_of(&mut self, cs: Vec<char>) -> String {
        let mut s = String::new();
        loop {
            let nc = self.peek();
            if nc.is_none() {
                break;
            }
            let nc = nc.unwrap();
            if cs.contains(&nc) {
                break;
            }
            s.push(nc);
            self.next(); // consume
        }
        s
    }

    fn read_until(&mut self, c: char) -> String {
        self.read_until_any_of(vec![c])
    }

    fn parse_block(&mut self, start: char, end: char) -> String {
        let mut s = String::new();
        let mut depth = 1;
        loop {
            let nc = self.next();
            if nc.is_none() {
                break;
            }
            let nc = nc.unwrap();
            if nc == start {
                depth += 1;
            } else if nc == end {
                depth -= 1;
            }
            if depth == 0 {
                break;
            }
            s.push(nc);
        }
        s
    }

    fn parse_comment(&mut self) {
        match self.expect_next_any_of(vec!['/', '*'], "parsing the beginning of a comment") {
            '/' => loop {
                let c = self.next();
                if c.is_none() {
                    break;
                }
                if c.unwrap() == '\n' {
                    break;
                }
            },
            '*' => loop {
                let c = self.next();
                if c.is_none() {
                    break;
                }
                if c.unwrap() == '*' {
                    let c = self.next();
                    if c.is_none() {
                        break;
                    }
                    if c.unwrap() == '/' {
                        break;
                    }
                }
            },
            _ => unreachable!(),
        }
    }

    fn parse_identifier(&mut self) -> String {
        let mut s = String::new();
        loop {
            let c = self.peek();
            if c.is_none() {
                break;
            }
            let c = c.unwrap();
            if c.is_alphanumeric() || c == '_' {
                s.push(self.expect("parsing an identifier"));
            } else {
                break;
            }
        }
        s
    }

    fn parse_type(&mut self) -> String {
        self.parse_identifier()
    }

    fn parse_string(&mut self) -> String {
        let mut s = String::new();
        loop {
            let c = self.next();
            if c.is_none() {
                break;
            }
            let c = c.unwrap();
            if c == '"' {
                break;
            }
            s.push(c);
        }
        s
    }

    /// Parse an include statement.
    ///
    /// ## Example
    /// ```
    /// include "path/to/file.webx";
    /// ```
    fn parse_include(&mut self) -> String {
        let context = "parsing an include statement";
        self.expect_specific_str("include", 1, context);
        self.expect_next_specific('"', context);
        let path = self.parse_string();
        let nc = self.next_skip_whitespace(false);
        self.expect_any_of(nc, vec!['\n', ';'], context);
        path
    }

    fn parse_location(&mut self) -> Result<WXScope, String> {
        let context = "parsing a location statement";
        self.expect_specific_str("location", 1, context);
        self.skip_whitespace(true);
        let path = self.parse_url_path();
        self.skip_whitespace(true);
        self.expect_next_specific('{', context);
        self.parse_scope(false, path)
    }

    fn parse_type_pair(&mut self) -> WXTypedIdentifier {
        let context = "parsing a type pair";
        self.skip_whitespace(true);
        let name = self.parse_identifier();
        self.skip_whitespace(true);
        self.expect_next_specific(':', context);
        self.skip_whitespace(true);
        let type_ = self.parse_type();
        self.skip_whitespace(true);
        WXTypedIdentifier { name, type_ }
    }

    fn parse_type_pairs(&mut self, allow_stray_comma: bool) -> Vec<WXTypedIdentifier> {
        let mut pairs = vec![];
        loop {
            let pair = self.parse_type_pair();
            if pair.name.is_empty() {
                break;
            } // Empty name means end of type pairs.
            pairs.push(pair);
            let nc = self.peek();
            if nc.is_none() {
                break;
            }
            let nc = nc.unwrap();
            if nc != ',' {
                break;
            } // No comma means end of type pairs.
            self.next(); // Consume the comma.
            self.skip_whitespace(true);
            if allow_stray_comma && !char::is_alphabetic(self.peek().unwrap()) {
                break;
            } // Allow stray comma.
        }
        pairs
    }

    fn parse_arguments(&mut self, end: char) -> Vec<String> {
        let mut args = vec![];
        loop {
            self.skip_whitespace(true);
            let arg = self.read_until_any_of(vec![',', end]).trim().to_string();
            if arg.is_empty() {
                break; // Empty name means end of arguments.
            }
            args.push(arg);
            let nc = self.peek();
            if nc.is_none() {
                break;
            }
            let nc = nc.unwrap();
            if nc != ',' {
                break; // No comma means end of arguments.
            }
            self.next(); // Consume the comma.
        }
        args
    }

    fn parse_model(&mut self) -> WXModel {
        let context = "parsing a model statement";
        self.expect_specific_str("model", 1, context);
        let name = self.read_until('{').trim().to_string();
        self.expect_next_specific('{', context);
        let fields = self.parse_type_pairs(true);
        self.expect_next_specific('}', context);
        WXModel { name, fields }
    }

    fn parse_code_body(&mut self) -> Option<WXBody> {
        self.skip_whitespace(true);
        match self.peek() {
            Some('{') => {
                self.next();
                Some(WXBody {
                    body_type: WXBodyType::TS,
                    body: self.parse_block('{', '}'),
                })
            }
            Some('(') => {
                self.next();
                Some(WXBody {
                    body_type: WXBodyType::TSX,
                    body: self.parse_block('(', ')'),
                })
            }
            _ => None,
        }
    }

    /// Parses a handler statement.
    ///
    /// ## Example
    /// ```ignore
    /// handler name(arg1: string, arg2: number) {
    ///    // ...
    /// }
    /// ```
    /// or
    /// ```ignore
    /// handler name(arg1: string, arg2: number) (
    ///     <h1>html</h1>
    /// )
    /// ```
    fn parse_handler(&mut self) -> WXHandler {
        let context = "parsing a handler statement";
        self.skip_whitespace(true);
        let name = self.read_until('(').trim().to_string();
        self.expect_next_specific('(', context);
        let params = self.parse_type_pairs(false);
        self.expect_next_specific(')', context);
        let body = self.parse_code_body();
        if body.is_none() {
            exit_error_unexpected(
                "handler body".to_string(),
                context,
                self.line,
                self.column,
                ERROR_SYNTAX,
            );
        }
        let body = body.unwrap();
        WXHandler { name, params, body }
    }

    /// Parse a URL path.
    /// ## Supporting syntax:
    /// - Static path segments
    /// - Dynamic path segments (arguments)
    /// - Optional path segments
    /// - Wildcard path segments
    /// - Regex path segments
    ///
    /// ## Example:
    /// ```ignore
    /// /path/to/(arg: string)?/*
    /// ```
    fn parse_url_path(&mut self) -> WXUrlPath {
        let context = "parsing a endpoint URL path";
        let mut segments: Vec<WXUrlPathSegment> = vec![];
        self.skip_whitespace(true);
        loop {
            match self.expect(context) {
                '(' => {
                    segments.push(WXUrlPathSegment::Parameter(self.parse_type_pair()));
                    self.expect_next_specific(')', context);
                }
                '*' => segments.push(WXUrlPathSegment::Regex("*".to_string())),
                '/' => {
                    let nc = self.peek();
                    if nc.is_some() && char::is_alphanumeric(nc.unwrap()) {
                        segments.push(WXUrlPathSegment::Literal(self.parse_identifier()));
                    }
                }
                c if c.is_alphabetic() => {
                    let mut name = c.to_string();
                    name.push_str(&self.parse_identifier());
                    segments.push(WXUrlPathSegment::Literal(name));
                }
                _ => break,
            }
        }
        WXUrlPath(segments)
    }

    /// Parse a request body format.
    /// ## Supporting syntax:
    /// - pre-defined formats (json, form, text, html)
    ///     - <name>(<field>: <type>, <field>: <type>, ...)
    /// - user-defined model name
    ///     - <name>
    ///
    /// ## Example:
    /// ```ignore
    /// json(text: string, n: number)
    /// form(name: string, age: number)
    /// User
    /// ```
    fn parse_body_format(&mut self) -> Option<WXRouteReqBody> {
        let context = "parsing a request body format";
        self.skip_whitespace(true);
        let nc = self.peek();
        if nc.is_some() && char::is_alphabetic(nc.unwrap()) {
            let name = self.parse_identifier();
            let nc = self.peek();
            if nc.is_some() && nc.unwrap() == '(' {
                // Custom format with fields.
                self.expect(context); // Consume the '('.
                let fields = self.parse_type_pairs(true);
                self.expect_next_specific(')', context);
                Some(WXRouteReqBody::Definition(name, fields))
            } else {
                // User-defined model name reference.
                Some(WXRouteReqBody::ModelReference(name))
            }
        } else {
            None
        }
    }

    fn parse_handler_call(&mut self) -> WXRouteHandler {
        let context = "parsing a handler call";
        let name = self.parse_identifier();
        self.expect_next_specific('(', context);
        let args = self.parse_arguments(')');
        self.expect_next_specific(')', context);
        self.skip_whitespace(true);
        let nc = self.peek();
        let output = if nc.is_some() && nc.unwrap() == ':' {
            self.expect_next_specific(':', context);
            self.skip_whitespace(true);
            Some(self.parse_identifier())
        } else {
            None
        };
        WXRouteHandler { name, args, output }
    }

    fn parse_route_handlers(&mut self) -> Vec<WXRouteHandler> {
        let context = "parsing route handlers";
        self.skip_whitespace(true);
        match self.peek() {
            Some('-') => {
                self.expect_specific_str("->", 0, context);
                let mut calls = vec![];
                loop {
                    self.skip_whitespace(true);
                    calls.push(self.parse_handler_call());
                    self.skip_whitespace(true);
                    let nc = self.peek();
                    if nc.is_none() {
                        break;
                    }
                    let nc = nc.unwrap();
                    if nc != ',' {
                        break;
                    }
                    self.next();
                }
                calls
            }
            _ => vec![],
        }
    }

    /// Parse a route statement.
    /// ## Supporting syntax:
    /// - HTTP method (get, post, put, patch, delete, connect, options, trace, head)
    /// - URL path with arguments
    /// - Request body format (json, form, text, html, or user-defined model)
    /// - Pre and post handlers
    /// - Response body
    ///     - TypeScript code (TS): Using `{}` delimiters
    ///     - HTML template (TSX): Using `()` delimiters
    ///
    /// ## Example:
    /// ```ignore
    /// get /path/to/route (<h1>My page</h1>)
    /// post /path/to/(arg: string)/route json(text: string, n: number) -> handler(arg, text) {
    ///     // ...
    /// }
    /// ```
    fn parse_route(&mut self, method: http::Method) -> Result<WXRoute, String> {
        Ok(WXRoute {
            info: WXInfoField { path: WXModulePath::new(self.file.clone()), line: self.line },
            method,
            path: self.parse_url_path(),
            body_format: self.parse_body_format(),
            pre_handlers: self.parse_route_handlers(),
            body: self.parse_code_body(),
            post_handlers: self.parse_route_handlers(),
        })
    }

    /// Parse either the global module scope, or a location scope.
    /// The function parses all basic components making up a webx
    /// module scope such as includes, nested locations, handlers,
    /// routes, and models.
    ///
    /// # Arguments
    /// * `is_global` - Whether the scope is global or not.
    fn parse_scope(&mut self, is_global: bool, path: WXUrlPath) -> Result<WXScope, String> {
        let context = "parsing a scope";
        let mut scope = WXScope {
            path,
            global_ts: String::new(),
            includes: vec![],
            models: vec![],
            handlers: vec![],
            routes: vec![],
            scopes: vec![],
        };
        loop {
            let c = self.next_skip_whitespace(true);
            if c.is_none() {
                // EOF is only allowed if the scope is global.
                if is_global {
                    break;
                } else {
                    exit_error_unexpected(
                        "EOF".to_string(),
                        context,
                        self.line,
                        self.column,
                        ERROR_SYNTAX,
                    );
                }
            }
            // Keywords: handler, include, location, module, { } and all HTTP methods.
            // Only expect a keyword at the start of a line, whitespace, or // comments.
            // Pass to dedicated parser function, otherwise error.
            let c = c.unwrap();
            match c {
                '}' => {
                    if is_global {
                        exit_error_unexpected_char(
                            '}',
                            context,
                            self.line,
                            self.column,
                            ERROR_SYNTAX,
                        );
                    } else {
                        break;
                    }
                }
                '/' => self.parse_comment(),
                'i' => scope.includes.push(self.parse_include()),
                'l' => scope.scopes.push(self.parse_location()?),
                'm' => scope.models.push(self.parse_model()),
                'h' => match self.expect(context) {
                    'a' => {
                        self.expect_specific_str("handler", 2, context);
                        scope.handlers.push(self.parse_handler());
                    }
                    'e' => {
                        self.expect_specific_str("head", 2, context);
                        scope.routes.push(self.parse_route(http::Method::HEAD)?);
                    }
                    c => exit_error_expected_any_of_but_found(
                        "handler or head".to_string(),
                        c,
                        context,
                        self.line,
                        self.column,
                        ERROR_SYNTAX,
                    ),
                },
                'g' => match self.expect(context) {
                    'e' => {
                        self.expect_specific_str("get", 2, context);
                        scope.routes.push(self.parse_route(http::Method::GET)?);
                    }
                    'l' => {
                        self.expect_specific_str("global", 2, context);
                        self.skip_whitespace(true);
                        self.expect_next_specific('{', context);
                        scope.global_ts = self.parse_block('{', '}');
                    }
                    c => exit_error_expected_any_of_but_found(
                        "get or global".to_string(),
                        c,
                        context,
                        self.line,
                        self.column,
                        ERROR_SYNTAX,
                    ),
                },
                'p' => match self.expect(context) {
                    'o' => {
                        self.expect_specific_str("post", 2, context);
                        scope.routes.push(self.parse_route(http::Method::POST)?);
                    }
                    'u' => {
                        self.expect_specific_str("put", 2, context);
                        scope.routes.push(self.parse_route(http::Method::PUT)?);
                    }
                    'a' => {
                        self.expect_specific_str("patch", 2, context);
                        scope.routes.push(self.parse_route(http::Method::PATCH)?);
                    }
                    c => exit_error_expected_any_of_but_found(
                        "post, put or patch".to_string(),
                        c,
                        context,
                        self.line,
                        self.column,
                        ERROR_SYNTAX,
                    ),
                },
                'd' => {
                    self.expect_specific_str("delete", 1, context);
                    scope.routes.push(self.parse_route(http::Method::DELETE)?);
                }
                'c' => {
                    self.expect_specific_str("connect", 1, context);
                    scope.routes.push(self.parse_route(http::Method::CONNECT)?);
                }
                'o' => {
                    self.expect_specific_str("options", 1, context);
                    scope.routes.push(self.parse_route(http::Method::OPTIONS)?);
                }
                't' => {
                    self.expect_specific_str("trace", 1, context);
                    scope.routes.push(self.parse_route(http::Method::TRACE)?);
                }
                _ => exit_error_unexpected_char(c, context, self.line, self.column, ERROR_SYNTAX),
            }
        }
        Ok(scope)
    }

    fn parse_module(&mut self) -> Result<WXModule, String> {
        Ok(WXModule {
            path: WXModulePath::new(self.file.clone()),
            scope: self.parse_scope(true, WXROOT_PATH)?,
        })
    }
}

pub fn parse_webx_file(file: &PathBuf) -> Result<WXModule, String> {
    let file_contents = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let mut parser = WebXFileParser::new(file, &file_contents);
    Ok(parser.parse_module()?)
}
