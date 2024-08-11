use crate::file::webx::WXModule;
use std::{
    fmt::Display,
    io::{BufReader, Read},
    path::PathBuf,
};

use super::webx::{
    WXBody, WXBodyType, WXHandler, WXInfoField, WXLiteralValue, WXModel, WXModulePath, WXRoute,
    WXRouteHandler, WXRouteReqBody, WXScope, WXTypedIdentifier, WXUrlPath, WXUrlPathSegment,
    WXROOT_PATH,
};

// ======================== Errors ========================

#[derive(Debug)]
pub enum WebXParserError {
    IoError(std::io::Error, PathBuf),
    SyntaxError(String, PathBuf),
}

impl WebXParserError {
    fn at_lc(msg: String, line: usize, column: usize) -> String {
        format!("{} at line {}, column {}", msg, line, column)
    }

    pub fn expected_but_found<T1: Display, T2: Display, T3: Display>(
        expected: T1,
        found: T2,
        context: T3,
        line: usize,
        column: usize,
        file: PathBuf,
    ) -> Self {
        WebXParserError::SyntaxError(
            Self::at_lc(
                format!(
                    "Expected {} but found '{}' while {}",
                    expected, found, context
                ),
                line,
                column,
            ),
            file,
        )
    }

    pub fn expected_any_of_but_found<T1: Display, T2: Display, T3: Display>(
        expected: &[T1],
        found: T2,
        context: T3,
        line: usize,
        column: usize,
        file: PathBuf,
    ) -> Self {
        let listing = expected
            .iter()
            .take(expected.len() - 1)
            .map(|e| e.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        let expected = if expected.len() > 1 {
            format!("{}, or {}", listing, expected.last().unwrap())
        } else {
            expected.first().unwrap().to_string()
        };
        WebXParserError::SyntaxError(
            Self::at_lc(
                format!(
                    "Expected any of {} but found '{}' while {}",
                    expected, found, context,
                ),
                line,
                column,
            ),
            file,
        )
    }

    pub fn unexpected<T1: Display, T2: Display>(
        what: T1,
        context: T2,
        line: usize,
        column: usize,
        file: PathBuf,
    ) -> Self {
        WebXParserError::SyntaxError(
            Self::at_lc(
                format!("Unexpected {} while {}", what, context),
                line,
                column,
            ),
            file,
        )
    }

    pub fn unexpected_char<T: Display>(
        what: char,
        context: T,
        line: usize,
        column: usize,
        file: PathBuf,
    ) -> Self {
        Self::unexpected(format!("character '{}'", what), context, line, column, file)
    }

    pub fn unexpected_eof<T: Display>(
        context: T,
        line: usize,
        column: usize,
        file: PathBuf,
    ) -> Self {
        Self::unexpected("EOF", context, line, column, file)
    }
}

// ======================== Parser ========================

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
        p.peeked = p.__raw_next().expect("Failed to read from file");
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
    fn __raw_next(&mut self) -> Result<Option<char>, WebXParserError> {
        let mut buf = [0; 1];
        let bytes_read = self
            .reader
            .read(&mut buf)
            .map_err(|err| WebXParserError::IoError(err, self.file.clone()))?;
        if bytes_read == 0 {
            return Ok(None);
        }
        let c = buf[0] as char;
        self.peeked_index += 1;
        // Index of the character returned by the next call to `next`.
        self.next_index = self.peeked_index - 1;
        Ok(Some(c))
    }

    fn peek(&self) -> Option<char> {
        self.peeked
    }
    fn next(&mut self) -> Result<Option<char>, WebXParserError> {
        let c = self.peeked;
        self.peeked = self.__raw_next()?;
        if let Some(c) = c {
            self.__update_line_column(c);
        }
        Ok(c)
    }
    fn expect(&mut self, context: &str) -> Result<char, WebXParserError> {
        let nc = self.next()?;
        self.expect_not_eof(nc, context)
    }

    fn expect_not_eof(&mut self, nc: Option<char>, context: &str) -> Result<char, WebXParserError> {
        match nc {
            Some(c) => Ok(c),
            None => Err(WebXParserError::unexpected_eof(
                context,
                self.line,
                self.column,
                self.file.clone(),
            )),
        }
    }

    /// Expect a specific character to be next in the file.
    /// Increments the line and column counters.
    ///
    /// # Errors
    /// If EOF is reached, or the next character is not the expected one, an error is returned and the program exits.
    fn expect_specific(
        &mut self,
        nc: Option<char>,
        expected: char,
        context: &str,
    ) -> Result<char, WebXParserError> {
        let nc = self.expect_not_eof(nc, context)?;
        if nc != expected {
            Err(WebXParserError::expected_but_found(
                expected,
                nc,
                context,
                self.line,
                self.column,
                self.file.clone(),
            ))
        } else {
            Ok(nc)
        }
    }

    fn expect_next_specific(
        &mut self,
        expected: char,
        context: &str,
    ) -> Result<char, WebXParserError> {
        let nc = self.next()?;
        self.expect_specific(nc, expected, context)
    }

    fn expect_specific_str(
        &mut self,
        expected: &str,
        already_read: usize,
        context: &str,
    ) -> Result<(), WebXParserError> {
        for c in expected.chars().skip(already_read) {
            if self.expect(context)? != c {
                return Err(WebXParserError::expected_but_found(
                    expected,
                    c,
                    context,
                    self.line,
                    self.column,
                    self.file.clone(),
                ));
            }
        }
        Ok(())
    }

    /// Expect any of the given characters to be next in the file.
    /// Increments the line and column counters.
    ///
    /// # Errors
    /// If EOF is reached, or the next character is not one of the expected ones, an error is returned and the program exits.
    fn expect_any_of(
        &mut self,
        nc: Option<char>,
        cs: Vec<char>,
        context: &str,
    ) -> Result<char, WebXParserError> {
        let nc = self.expect_not_eof(nc, context)?;
        if !cs.contains(&nc) {
            return Err(WebXParserError::expected_any_of_but_found(
                &cs,
                nc,
                context,
                self.line,
                self.column,
                self.file.clone(),
            ));
        }
        Ok(nc)
    }

    fn expect_next_any_of(
        &mut self,
        cs: Vec<char>,
        context: &str,
    ) -> Result<char, WebXParserError> {
        let nc = self.next()?;
        self.expect_any_of(nc, cs, context)
    }

    fn skip_whitespace(&mut self, skip_newlines: bool) {
        loop {
            let c = self.peek();
            if c.is_none() {
                break;
            }
            let c = c.unwrap();
            if c == ' ' || c == '\t' || c == '\r' || (skip_newlines && c == '\n') {
                self.next().expect("Failed to skip whitespace");
            } else {
                break;
            }
        }
    }

    fn next_skip_whitespace(
        &mut self,
        skip_newlines: bool,
    ) -> Result<Option<char>, WebXParserError> {
        loop {
            let c = self.next()?;
            if c.is_none() {
                break;
            }
            let c = c.unwrap();
            if c == ' ' || c == '\t' || c == '\r' || (skip_newlines && c == '\n') {
                continue;
            }
            return Ok(Some(c)); // Return the first non-whitespace character.
        }
        Ok(None)
    }

    fn read_while<F: Fn(char) -> bool>(&mut self, f: F) -> Result<String, WebXParserError> {
        let mut s = String::new();
        loop {
            let nc = self.peek();
            if nc.is_none() {
                break;
            }
            let nc = nc.unwrap();
            if f(nc) {
                s.push(nc);
                self.next()?; // consume
            } else {
                break;
            }
        }
        Ok(s)
    }

    fn read_until_any_of(&mut self, cs: Vec<char>) -> Result<String, WebXParserError> {
        self.read_while(|c| !cs.contains(&c))
    }

    fn read_until(&mut self, c: char) -> Result<String, WebXParserError> {
        self.read_until_any_of(vec![c])
    }

    fn parse_block(&mut self, start: char, end: char) -> Result<String, WebXParserError> {
        let mut s = String::new();
        let mut depth = 1;
        loop {
            let nc = self.next()?;
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
        Ok(s)
    }

    fn parse_comment(&mut self) -> Result<(), WebXParserError> {
        match self.expect_next_any_of(vec!['/', '*'], "parsing the beginning of a comment")? {
            '/' => loop {
                let c = self.next()?;
                if c.is_none() {
                    break;
                }
                if c.unwrap() == '\n' {
                    break;
                }
            },
            '*' => loop {
                let c = self.next()?;
                if c.is_none() {
                    break;
                }
                if c.unwrap() == '*' {
                    let c = self.next()?;
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
        Ok(())
    }

    fn parse_identifier(&mut self) -> Result<String, WebXParserError> {
        let mut s = String::new();
        loop {
            let c = self.peek();
            if c.is_none() {
                break;
            }
            let c = c.unwrap();
            if c.is_alphanumeric() || c == '_' {
                s.push(self.expect("parsing an identifier")?);
            } else {
                break;
            }
        }
        Ok(s)
    }

    fn parse_type(&mut self) -> Result<String, WebXParserError> {
        self.parse_identifier()
    }

    fn parse_string(&mut self) -> Result<String, WebXParserError> {
        let mut s = String::new();
        loop {
            let c = self.next()?;
            if c.is_none() {
                break;
            }
            let c = c.unwrap();
            if c == '"' {
                break;
            }
            s.push(c);
        }
        Ok(s)
    }

    /// Parse an include statement.
    ///
    /// ## Example
    /// ```
    /// include "path/to/file.webx";
    /// ```
    fn parse_include(&mut self) -> Result<String, WebXParserError> {
        let context = "parsing an include statement";
        self.expect_specific_str("include", 1, context)?;
        self.expect_next_specific('"', context)?;
        let path = self.parse_string()?;
        let nc = self.next_skip_whitespace(false)?;
        self.expect_any_of(nc, vec!['\n', ';'], context)?;
        Ok(path)
    }

    fn parse_location(&mut self) -> Result<WXScope, WebXParserError> {
        let context = "parsing a location statement";
        self.expect_specific_str("location", 1, context)?;
        self.skip_whitespace(true);
        let path = self.parse_url_path()?;
        self.skip_whitespace(true);
        self.expect_next_specific('{', context)?;
        self.parse_scope(false, path)
    }

    fn parse_type_pair(&mut self) -> Result<WXTypedIdentifier, WebXParserError> {
        let context = "parsing a type pair";
        self.skip_whitespace(true);
        let name = self.parse_identifier()?;
        self.skip_whitespace(true);
        self.expect_next_specific(':', context)?;
        self.skip_whitespace(true);
        let type_ = self.parse_type()?;
        self.skip_whitespace(true);
        Ok(WXTypedIdentifier { name, type_ })
    }

    fn parse_type_pairs(
        &mut self,
        allow_stray_comma: bool,
    ) -> Result<Vec<WXTypedIdentifier>, WebXParserError> {
        let mut pairs = vec![];
        loop {
            let pair = self.parse_type_pair()?;
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
            self.next()?; // Consume the comma.
            self.skip_whitespace(true);
            if allow_stray_comma && !char::is_alphabetic(self.peek().unwrap()) {
                break;
            } // Allow stray comma.
        }
        Ok(pairs)
    }

    fn parse_literal(&mut self) -> Result<WXLiteralValue, WebXParserError> {
        let context = "parsing a literal value";
        self.skip_whitespace(true);
        let nc = self.peek();
        if nc.is_none() {
            return Err(WebXParserError::unexpected_eof(
                context,
                self.line,
                self.column,
                self.file.clone(),
            ));
        }
        let nc = nc.unwrap();
        Ok(match nc {
            '"' => {
                self.expect_next_specific('"', context)?;
                WXLiteralValue::String(self.parse_string()?)
            }
            '[' => {
                self.expect_next_specific('[', context)?;
                let mut values = vec![];
                loop {
                    values.push(self.parse_literal()?);
                    let nc = self.expect_next_any_of(vec![',', ']'], context)?;
                    if nc == ']' {
                        break;
                    }
                    self.next()?; // Consume the comma.
                }
                WXLiteralValue::Array(values)
            }
            '{' => {
                self.expect_next_specific('{', context)?;
                let mut values = vec![];
                loop {
                    let name = self.parse_identifier()?;
                    self.expect_next_specific(':', context)?;
                    let value = self.parse_literal()?;
                    values.push((name, value));
                    let nc = self.expect_next_any_of(vec![',', '}'], context)?;
                    if nc == '}' {
                        break;
                    }
                    self.next()?; // Consume the comma.
                }
                WXLiteralValue::Object(values)
            }
            c if c.is_numeric() => {
                let integer = self.read_while(|c| c.is_numeric())?;
                let mut fraction = "0".to_string();
                if self.peek().is_some() && self.peek().unwrap() == '.' {
                    self.next()?; // Consume the dot.
                    fraction = self.read_while(|c| c.is_numeric())?;
                }
                WXLiteralValue::Number(
                    integer.parse::<u32>().unwrap(),
                    fraction.parse::<u32>().unwrap(),
                )
            }
            c if c.is_alphabetic() => {
                let name = self.parse_identifier()?;
                if name == "true" {
                    WXLiteralValue::Boolean(true)
                } else if name == "false" {
                    WXLiteralValue::Boolean(false)
                } else if name == "null" {
                    WXLiteralValue::Null
                } else {
                    WXLiteralValue::Identifier(name)
                }
            }
            _ => {
                return Err(WebXParserError::unexpected_char(
                    nc,
                    context,
                    self.line,
                    self.column,
                    self.file.clone(),
                ))
            }
        })
    }

    fn parse_arguments(&mut self, end: char) -> Result<Vec<String>, WebXParserError> {
        let mut args = vec![];
        if let Some(nc) = self.peek() {
            if nc == end {
                return Ok(args);
            } // Empty arguments.
        }
        loop {
            match self.parse_literal()? {
                WXLiteralValue::Identifier(name) => args.push(name),
                other => {
                    return Err(WebXParserError::expected_but_found(
                        "identifier",
                        other.to_string(),
                        "parsing arguments",
                        self.line,
                        self.column,
                        self.file.clone(),
                    ))
                }
            }
            if let Some(nc) = self.peek() {
                if nc == end {
                    break; // End of arguments.
                } else if nc == ',' {
                    self.next()?; // Consume the comma.
                } else {
                    return Err(WebXParserError::expected_any_of_but_found(
                        &[',', end],
                        nc,
                        "parsing arguments",
                        self.line,
                        self.column,
                        self.file.clone(),
                    ));
                }
            } else {
                return Err(WebXParserError::unexpected_eof(
                    "parsing arguments",
                    self.line,
                    self.column,
                    self.file.clone(),
                ));
            }
        }
        Ok(args)
    }

    fn parse_model(&mut self) -> Result<WXModel, WebXParserError> {
        let context = "parsing a model statement";
        self.expect_specific_str("model", 1, context)?;
        let name = self.read_until('{')?.trim().to_string();
        self.expect_next_specific('{', context)?;
        let fields = self.parse_type_pairs(true)?;
        self.expect_next_specific('}', context)?;
        Ok(WXModel { name, fields })
    }

    fn de_indent_block(s: String) -> String {
        let initial_indent = s
            .lines()
            .last()
            .unwrap_or("")
            .chars()
            .take_while(|c| c.is_whitespace())
            .count();
        s.lines()
            .map(|l| {
                let line_indent = l.chars().take_while(|c| c.is_whitespace()).count();
                if line_indent >= initial_indent {
                    l.chars().skip(initial_indent).collect::<String>()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<String>>()
            .join("\n")
    }

    fn parse_code_body(&mut self) -> Result<Option<WXBody>, WebXParserError> {
        self.skip_whitespace(true);
        Ok(match self.peek() {
            Some('{') => {
                self.next()?;
                Some(WXBody {
                    body_type: WXBodyType::Ts,
                    body: Self::de_indent_block(self.parse_block('{', '}')?),
                })
            }
            Some('(') => {
                self.next()?;
                Some(WXBody {
                    body_type: WXBodyType::Tsx,
                    body: Self::de_indent_block(self.parse_block('(', ')')?),
                })
            }
            _ => None,
        })
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
    fn parse_handler(&mut self) -> Result<WXHandler, WebXParserError> {
        let context = "parsing a handler statement";
        self.skip_whitespace(true);
        let name = self.read_until('(')?.trim().to_string();
        self.expect_next_specific('(', context)?;
        let params = self.parse_type_pairs(false)?;
        self.expect_next_specific(')', context)?;
        let body = self.parse_code_body()?;
        if body.is_none() {
            return Err(WebXParserError::unexpected(
                "handler body",
                context,
                self.line,
                self.column,
                self.file.clone(),
            ));
        }
        let body = body.unwrap();
        Ok(WXHandler { name, params, body })
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
    fn parse_url_path(&mut self) -> Result<WXUrlPath, WebXParserError> {
        let context = "parsing an endpoint URL path";
        let mut segments: Vec<WXUrlPathSegment> = vec![];
        self.skip_whitespace(true);
        let mut regex_counter = 0;
        loop {
            match self.expect(context)? {
                '(' => {
                    segments.push(WXUrlPathSegment::Parameter(self.parse_type_pair()?));
                    self.expect_next_specific(')', context)?;
                }
                '*' => {
                    segments.push(WXUrlPathSegment::Regex(
                        format!("g{}", regex_counter),
                        "*".to_string(),
                    ));
                    regex_counter += 1;
                }
                '/' => {
                    let nc = self.peek();
                    if let Some(nc) = nc {
                        if nc.is_alphanumeric() {
                            segments.push(WXUrlPathSegment::Literal(self.parse_identifier()?));
                        } else if nc.is_whitespace() {
                            // Allow root path to be empty. E.g. `get / ... `.
                            segments.push(WXUrlPathSegment::Literal("".to_string()));
                        }
                    }
                }
                c if c.is_alphabetic() => {
                    let mut name = c.to_string();
                    name.push_str(&self.parse_identifier()?);
                    segments.push(WXUrlPathSegment::Literal(name));
                }
                _ => break,
            }
        }
        // Remove all empty segments.
        segments.retain(|s| {
            if let WXUrlPathSegment::Literal(s) = s {
                !s.is_empty()
            } else {
                true
            }
        });
        Ok(WXUrlPath(segments))
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
    fn parse_body_format(&mut self) -> Result<Option<WXRouteReqBody>, WebXParserError> {
        let context = "parsing a request body format";
        self.skip_whitespace(true);
        let nc = self.peek();
        Ok(if nc.is_some() && char::is_alphabetic(nc.unwrap()) {
            let name = self.parse_identifier()?;
            let nc = self.peek();
            if nc.is_some() && nc.unwrap() == '(' {
                // Custom format with fields.
                self.expect(context)?; // Consume the '('.
                let fields = self.parse_type_pairs(true)?;
                self.expect_next_specific(')', context)?;
                Some(WXRouteReqBody::Definition(name, fields))
            } else {
                // User-defined model name reference.
                Some(WXRouteReqBody::ModelReference(name))
            }
        } else {
            None
        })
    }

    fn parse_handler_call(&mut self) -> Result<WXRouteHandler, WebXParserError> {
        let context = "parsing a handler call";
        let name = self.parse_identifier()?;
        self.expect_next_specific('(', context)?;
        let args = self.parse_arguments(')')?;
        self.expect_next_specific(')', context)?;
        self.skip_whitespace(true);
        let nc = self.peek();
        let output = if nc.is_some() && nc.unwrap() == ':' {
            self.expect_next_specific(':', context)?;
            self.skip_whitespace(true);
            Some(self.parse_identifier()?)
        } else {
            None
        };
        Ok(WXRouteHandler { name, args, output })
    }

    fn parse_route_handlers(&mut self) -> Result<Vec<WXRouteHandler>, WebXParserError> {
        let context = "parsing route handlers";
        self.skip_whitespace(true);
        Ok(match self.peek() {
            Some('-') => {
                self.expect_specific_str("->", 0, context)?;
                let mut calls = vec![];
                loop {
                    self.skip_whitespace(true);
                    calls.push(self.parse_handler_call()?);
                    self.skip_whitespace(true);
                    let nc = self.peek();
                    if nc.is_none() {
                        break;
                    }
                    let nc = nc.unwrap();
                    if nc != ',' {
                        break;
                    }
                    self.next()?;
                }
                calls
            }
            _ => vec![],
        })
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
    fn parse_route(&mut self, method: hyper::Method) -> Result<WXRoute, WebXParserError> {
        Ok(WXRoute {
            info: WXInfoField {
                path: WXModulePath::new(self.file.clone()),
                line: self.line,
            },
            method,
            path: self.parse_url_path()?,
            body_format: self.parse_body_format()?,
            pre_handlers: self.parse_route_handlers()?,
            body: self.parse_code_body()?,
            post_handlers: self.parse_route_handlers()?,
        })
    }

    /// Parse either the global module scope, or a location scope.
    /// The function parses all basic components making up a webx
    /// module scope such as includes, nested locations, handlers,
    /// routes, and models.
    ///
    /// # Arguments
    /// * `is_global` - Whether the scope is global or not.
    fn parse_scope(
        &mut self,
        is_global: bool,
        path: WXUrlPath,
    ) -> Result<WXScope, WebXParserError> {
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
            let c = self.next_skip_whitespace(true)?;
            if c.is_none() {
                // EOF is only allowed if the scope is global.
                if is_global {
                    break;
                } else {
                    return Err(WebXParserError::unexpected_eof(
                        context,
                        self.line,
                        self.column,
                        self.file.clone(),
                    ));
                }
            }
            // Keywords: handler, include, location, module, { } and all HTTP methods.
            // Only expect a keyword at the start of a line, whitespace, or // comments.
            // Pass to dedicated parser function, otherwise error.
            let c = c.unwrap();
            match c {
                '}' => {
                    if is_global {
                        return Err(WebXParserError::unexpected_char(
                            '}',
                            context,
                            self.line,
                            self.column,
                            self.file.clone(),
                        ));
                    } else {
                        break;
                    }
                }
                '/' => self.parse_comment()?,
                'i' => scope.includes.push(self.parse_include()?),
                'l' => scope.scopes.push(self.parse_location()?),
                'm' => scope.models.push(self.parse_model()?),
                'h' => match self.expect(context)? {
                    'a' => {
                        self.expect_specific_str("handler", 2, context)?;
                        scope.handlers.push(self.parse_handler()?);
                    }
                    'e' => {
                        self.expect_specific_str("head", 2, context)?;
                        scope.routes.push(self.parse_route(hyper::Method::HEAD)?);
                    }
                    c => {
                        return Err(WebXParserError::expected_any_of_but_found(
                            &["handler", "head"],
                            c,
                            context,
                            self.line,
                            self.column,
                            self.file.clone(),
                        ))
                    }
                },
                'g' => match self.expect(context)? {
                    'e' => {
                        self.expect_specific_str("get", 2, context)?;
                        scope.routes.push(self.parse_route(hyper::Method::GET)?);
                    }
                    'l' => {
                        self.expect_specific_str("global", 2, context)?;
                        self.skip_whitespace(true);
                        self.expect_next_specific('{', context)?;
                        scope.global_ts = self.parse_block('{', '}')?;
                    }
                    c => {
                        return Err(WebXParserError::expected_any_of_but_found(
                            &["get", "global"],
                            c,
                            context,
                            self.line,
                            self.column,
                            self.file.clone(),
                        ))
                    }
                },
                'p' => match self.expect(context)? {
                    'o' => {
                        self.expect_specific_str("post", 2, context)?;
                        scope.routes.push(self.parse_route(hyper::Method::POST)?);
                    }
                    'u' => {
                        self.expect_specific_str("put", 2, context)?;
                        scope.routes.push(self.parse_route(hyper::Method::PUT)?);
                    }
                    'a' => {
                        self.expect_specific_str("patch", 2, context)?;
                        scope.routes.push(self.parse_route(hyper::Method::PATCH)?);
                    }
                    c => {
                        return Err(WebXParserError::expected_any_of_but_found(
                            &["post", "put", "patch"],
                            c,
                            context,
                            self.line,
                            self.column,
                            self.file.clone(),
                        ))
                    }
                },
                'd' => {
                    self.expect_specific_str("delete", 1, context)?;
                    scope.routes.push(self.parse_route(hyper::Method::DELETE)?);
                }
                'c' => {
                    self.expect_specific_str("connect", 1, context)?;
                    scope.routes.push(self.parse_route(hyper::Method::CONNECT)?);
                }
                'o' => {
                    self.expect_specific_str("options", 1, context)?;
                    scope.routes.push(self.parse_route(hyper::Method::OPTIONS)?);
                }
                't' => {
                    self.expect_specific_str("trace", 1, context)?;
                    scope.routes.push(self.parse_route(hyper::Method::TRACE)?);
                }
                _ => {
                    return Err(WebXParserError::unexpected_char(
                        c,
                        context,
                        self.line,
                        self.column,
                        self.file.clone(),
                    ))
                }
            }
        }
        Ok(scope)
    }

    fn parse_module(&mut self) -> Result<WXModule, WebXParserError> {
        Ok(WXModule {
            path: WXModulePath::new(self.file.clone()),
            scope: self.parse_scope(true, WXROOT_PATH)?,
        })
    }
}

pub fn parse_webx_file(file: &PathBuf) -> Result<WXModule, WebXParserError> {
    let file_contents =
        std::fs::read_to_string(file).map_err(|err| WebXParserError::IoError(err, file.clone()))?;
    let mut parser = WebXFileParser::new(file, &file_contents);
    parser.parse_module()
}
