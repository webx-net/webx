use std::{path::PathBuf, io::{BufReader, Read}};
use crate::{file::webx::WebXFile, reporting::error::{exit_error, ERROR_PARSE_IO, ERROR_SYNTAX, exit_error_unexpected_char, exit_error_unexpected, exit_error_expected_any_of_but_found, exit_error_expected_but_found}};

use super::webx::{WebXScope, WebXModel, WebXRouteMethod, WebXRoute};

struct WebXFileParser<'a> {
    file: &'a PathBuf,
    _content: &'a String,
    reader: BufReader<&'a [u8]>,
    line: usize,
    column: usize,
    peeked_index: u64, // "next index"
    next_index: u64, // "current index"
    peeked: Option<char>,
}

impl<'a> WebXFileParser<'a> {
    fn new(file: &'a PathBuf, content: &'a String) -> WebXFileParser<'a> {
        let mut p = WebXFileParser {
            file,
            _content: content,
            reader: BufReader::new(content.as_bytes()),
            line: 0,
            column: 0,
            peeked_index: 0,
            next_index: 0,
            peeked: None,
        };
        p.peeked = p.__raw_next();
        p
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
            Err(e) => exit_error(format!("Failed to read file '{}' due to, {}", self.file.display(), e), ERROR_PARSE_IO),
        };
        if bytes_read == 0 { return None; }
        let c = buf[0] as char;
        self.peeked_index += 1;
        // Index of the character returned by the next call to `next`.
        self.next_index = self.peeked_index - 1;
        if c == '\n' {
            self.line += 1;
            self.column = 0;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn peek(&self) -> Option<char> { self.peeked }
    fn next(&mut self) -> Option<char> {
        let c = self.peeked;
        self.peeked = self.__raw_next();
        c
    }
    fn expect(&mut self) -> char {
        let nc = self.next();
        self.expect_not_eof(nc)
    }

    fn expect_not_eof(&mut self, nc: Option<char>) -> char {
        if nc.is_none() { exit_error_unexpected("EOF".to_string(), self.line, self.column, ERROR_SYNTAX); }
        nc.unwrap()
    }

    /// Expect a specific character to be next in the file.
    /// Increments the line and column counters.
    /// 
    /// # Errors
    /// If EOF is reached, or the next character is not the expected one, an error is returned and the program exits.
    fn expect_specific(&mut self, nc: Option<char>, expected: char) -> char {
        let nc = self.expect_not_eof(nc);
        if nc != expected {
            exit_error_expected_but_found(expected.to_string(), nc.to_string(), self.line, self.column, ERROR_SYNTAX);
        }
        nc
    }

    fn expect_next_specific(&mut self, expected: char) -> char {
        let nc = self.next();
        self.expect_specific(nc, expected)
    }

    fn expect_specific_str(&mut self, expected: &str, already_read: usize) {
        for c in expected.chars().skip(already_read) {
            if self.expect() != c {
                exit_error_expected_but_found(expected.to_string(), c.to_string(), self.line, self.column, ERROR_SYNTAX);
            }
        }
    }

    /// Expect any of the given characters to be next in the file.
    /// Increments the line and column counters.
    /// 
    /// # Errors
    /// If EOF is reached, or the next character is not one of the expected ones, an error is returned and the program exits.
    fn expect_any_of(&mut self, nc: Option<char>, cs: Vec<char>) -> char {
        let nc = self.expect_not_eof(nc);
        if !cs.contains(&nc) {
            exit_error_expected_any_of_but_found(format!("{:?}", cs), nc, self.line, self.column, ERROR_SYNTAX);
        }
        nc
    }

    fn expect_next_any_of(&mut self, cs: Vec<char>) -> char {
        let nc = self.next();
        self.expect_any_of(nc, cs)
    }

    fn skip_whitespace(&mut self, skip_newlines: bool) {
        loop {
            let c = self.peek();
            if c.is_none() { break; }
            let c = c.unwrap();
            if c == ' ' || c == '\t' || (skip_newlines && c == '\n') { self.next(); }
            else { break; }
        }
    }

    fn next_skip_whitespace(&mut self, skip_newlines: bool) -> Option<char> {
        loop {
            let c = self.next();
            if c.is_none() { break; }
            let c = c.unwrap();
            if c == ' ' || c == '\t' || (skip_newlines && c == '\n') { continue; }
            return Some(c); // Return the first non-whitespace character.
        }
        None
    }

    fn read_until_any_of(&mut self, cs: Vec<char>) -> String {
        let mut s = String::new();
        loop {
            let nc = self.peek();
            if nc.is_none() { break; }
            let nc = nc.unwrap();
            if cs.contains(&nc) { break; }
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
            if nc.is_none() { break; }
            let nc = nc.unwrap();
            if nc == start { depth += 1; }
            else if nc == end { depth -= 1; }
            if depth == 0 { break; }
            s.push(nc);
        }
        s
    }

    fn parse_comment(&mut self) {
        match self.expect_next_any_of(vec!['/', '*']) {
            '/' => {
                loop {
                    let c = self.next();
                    if c.is_none() { break; }
                    if c.unwrap() == '\n' { break; }
                }
            },
            '*' => {
                loop {
                    let c = self.next();
                    if c.is_none() { break; }
                    if c.unwrap() == '*' {
                        let c = self.next();
                        if c.is_none() { break; }
                        if c.unwrap() == '/' { break; }
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
            if c.is_none() { break; }
            let c = c.unwrap();
            if c.is_alphanumeric() || c == '_' { s.push(self.expect()); }
            else { break; }
        }
        s
    }

    fn parse_string(&mut self) -> String {
        let mut s = String::new();
        loop {
            let c = self.next();
            if c.is_none() { break; }
            let c = c.unwrap();
            if c == '"' { break; }
            s.push(c);
        }
        s
    }

    /// Parse an include statement.
    /// 
    /// # Example
    /// ```
    /// include "path/to/file.webx";
    /// ```
    fn parse_include(&mut self) -> String {
        self.expect_specific_str("include", 1);
        self.expect_next_specific('"');
        let path = self.parse_string();
        let nc = self.next_skip_whitespace(false);
        self.expect_any_of(nc, vec!['\n', ';']);
        path
    }

    fn parse_location(&mut self) -> Result<WebXScope, String> {
        self.expect_specific_str("location", 1);
        let nc = self.next_skip_whitespace(false);
        self.expect_specific(nc, '{');
        self.parse_scope(false)
    }

    fn parse_model(&mut self) -> WebXModel {
        self.expect_specific_str("model", 1);
        let name = self.read_until('{');
        let fields = self.read_until('}');
        WebXModel { name, fields }
    }

    fn parse_handler(&mut self) {
        todo!("parse_handler")
    }

    /// Parse a URL path variable segment.
    /// **Obs: This function does not parse the opening parenthesis.** 
    /// 
    /// ## Example:
    /// ```ignore
    /// (arg: string)?
    /// ```
    fn parse_url_path_variable(&mut self) -> String {
        let name = self.read_until(':').trim().to_string();
        self.expect_next_specific(':');
        let type_ = self.read_until(')').trim().to_string();
        self.expect_next_specific(')');
        format!("({}:{})", name, type_)
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
    fn parse_url_path(&mut self) -> String {
        let mut path = String::new();
        let c = self.next_skip_whitespace(true);
        if c.is_none() { exit_error_expected_but_found("endpoint path".to_string(), "EOF".to_string(), self.line, self.column, ERROR_SYNTAX); }
        let mut c = c.unwrap();
        loop {
            match c {
                '(' => path.push_str(&self.parse_url_path_variable()),
                c if c.is_alphanumeric()
                        || c == '/'
                        || c == '_'
                        || c == '*'
                        => path.push(c),
                _ => break,
            }
            c = self.next().unwrap();
        }
        path
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
    fn parse_body_format(&mut self) -> Option<String> {
        self.skip_whitespace(true);
        let mut result = self.read_until(')');
        result.push(self.expect_next_specific(')'));
        Some(result)
    }

    fn parse_handler_call(&mut self) -> String {
        // parse "handler(arg1, arg2, ...): output"
        let mut call = self.read_until(')');
        call.push(self.expect_next_specific(')'));
        // Optional output value
        self.skip_whitespace(true);
        let nc = self.peek();
        if nc.is_some() && nc.unwrap() == ':' {
            call.push(self.next().unwrap());
            self.skip_whitespace(true);
            call.push_str(self.parse_identifier().as_str());
        }
        call
    }

    fn parse_handler_calls(&mut self) -> Vec<String> {
        let mut calls = vec![];
        loop {
            self.skip_whitespace(true);
            calls.push(self.parse_handler_call());
            self.skip_whitespace(true);
            let nc = self.peek();
            if nc.is_none() { break; }
            let nc = nc.unwrap();
            if nc != ',' { break; }
            self.next();
        }
        calls
    }

    fn parse_route_handlers(&mut self) -> Vec<String> {
        self.skip_whitespace(true);
        match self.peek() {
            Some('-') => {
                self.expect_specific_str("->", 0);
                self.parse_handler_calls()
            },
            _ => vec![]
        }
    }

    fn parse_route_body(&mut self) -> Option<String> {
        match self.next_skip_whitespace(true) {
            Some('{') => Some(self.parse_block('{', '}')),
            Some('(') => Some(self.parse_block('(', ')')),
            _ => None,
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
    fn parse_route(&mut self, method: WebXRouteMethod) -> Result<WebXRoute, String> {
        Ok(WebXRoute {
            method,
            path: self.parse_url_path(),
            body_format: self.parse_body_format(),
            pre_handlers: self.parse_route_handlers(),
            body: self.parse_route_body(),
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
    fn parse_scope(&mut self, is_global: bool) -> Result<WebXScope, String> {
        let mut scope = WebXScope {
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
                if is_global { break; }
                else { exit_error_unexpected("EOF".to_string(), self.line, self.column, ERROR_SYNTAX); }
            }
            // Keywords: handler, include, location, module, { } and all HTTP methods.
            // Only expect a keyword at the start of a line, whitespace, or // comments.
            // Pass to dedicated parser function, otherwise error.
            let c = c.unwrap();
            match c {
                '}' => {
                    if is_global { exit_error_unexpected_char('}', self.line, self.column, ERROR_SYNTAX); }
                    else { break; }
                },
                '/' => self.parse_comment(),
                'i' => scope.includes.push(self.parse_include()),
                'l' => scope.scopes.push(self.parse_location()?),
                'm' => scope.models.push(self.parse_model()),
                'h' => match self.expect() {
                    'a' => self.parse_handler(),
                    'e' => {
                        self.expect_specific_str("head", 2);
                        scope.routes.push(self.parse_route(WebXRouteMethod::HEAD)?);
                    },
                    c => exit_error_expected_any_of_but_found("handler or head".to_string(), c, self.line, self.column, ERROR_SYNTAX),
                },
                'g' => match self.expect() {
                    'e' => {
                        self.expect_specific_str("get", 2);
                        scope.routes.push(self.parse_route(WebXRouteMethod::GET)?);
                    },
                    'l' => {
                        self.expect_specific_str("global", 2);
                        scope.global_ts = self.parse_block('{', '}');
                    },
                    c => exit_error_expected_any_of_but_found("get or global".to_string(), c, self.line, self.column, ERROR_SYNTAX),
                },
                'p' => match self.expect() {
                    'o' => {
                        self.expect_specific_str("post", 2);
                        scope.routes.push(self.parse_route(WebXRouteMethod::POST)?);
                    },
                    'u' => {
                        self.expect_specific_str("put", 2);
                        scope.routes.push(self.parse_route(WebXRouteMethod::PUT)?);
                    },
                    'a' => {
                        self.expect_specific_str("patch", 2);
                        scope.routes.push(self.parse_route(WebXRouteMethod::PATCH)?);
                    },
                    c => exit_error_expected_any_of_but_found("post, put or patch".to_string(), c, self.line, self.column, ERROR_SYNTAX),
                },
                'd' => {
                    self.expect_specific_str("delete", 1);
                    scope.routes.push(self.parse_route(WebXRouteMethod::DELETE)?);
                },
                'c' => {
                    self.expect_specific_str("connect", 1);
                    scope.routes.push(self.parse_route(WebXRouteMethod::CONNECT)?);
                },
                'o' => {
                    self.expect_specific_str("options", 1);
                    scope.routes.push(self.parse_route(WebXRouteMethod::OPTIONS)?);
                },
                't' => {
                    self.expect_specific_str("trace", 1);
                    scope.routes.push(self.parse_route(WebXRouteMethod::TRACE)?);
                }
                _ => exit_error_unexpected_char(c, self.line, self.column, ERROR_SYNTAX),
            }
        }
        Ok(scope)
    }

    fn parse_module(&mut self) -> Result<WebXFile, String> {
        Ok(WebXFile {
            path: self.file.clone(),
            module_scope: self.parse_scope(true)?,
        })
    }
}

pub fn parse_webx_file(file: &PathBuf) -> Result<WebXFile, String> {
    let file_contents = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let mut parser = WebXFileParser::new(file, &file_contents);
    Ok(parser.parse_module()?)
}
