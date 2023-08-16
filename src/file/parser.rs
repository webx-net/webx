use std::{path::PathBuf, io::{BufReader, Read, Seek, SeekFrom}};
use crate::{file::webx::WebXFile, reporting::error::{exit_error, ERROR_PARSE_IO, ERROR_SYNTAX, exit_error_unexpected_char, exit_error_unexpected, exit_error_expected_any_of_but_found, exit_error_expected_but_found}};

use super::webx::WebXScope;

struct WebXFileParser<'a> {
    file: &'a PathBuf,
    content: &'a String,
    reader: BufReader<&'static [u8]>,
    line: usize,
    column: usize,
    index: u64,
}

impl<'a> WebXFileParser<'a> {
    fn new(file: &'a PathBuf, content: &'a String) -> WebXFileParser<'a> {
        WebXFileParser {
            file,
            content,
            reader: BufReader::new(content.as_bytes()),
            line: 0,
            column: 0,
            index: 0,
        }
    }

    /// Returns the next character in the file, or None if EOF is reached.
    /// Increments the line and column counters.
    /// 
    /// # Errors
    /// If the file cannot be read, an error is returned and the program exits.
    fn next(&mut self) -> Option<char> {
        let mut buf = [0; 1];
        let bytes_read = match self.reader.read(&mut buf) {
            Ok(n) => n,
            Err(e) => exit_error(format!("Failed to read file '{}' due to, {}", self.file.display(), e), ERROR_PARSE_IO),
        };
        if bytes_read == 0 { return None; }
        let c = buf[0] as char;
        self.index += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 0;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    /// Expect a specific character to be next in the file.
    /// Increments the line and column counters.
    /// 
    /// # Errors
    /// If EOF is reached, or the next character is not the expected one, an error is returned and the program exits.
    fn expect_specific(&mut self, nc: Option<char>, expected: char) {
        if nc.is_none() {
            exit_error_unexpected("EOF".to_string(), self.line, self.column, ERROR_SYNTAX);
        } else if nc.unwrap() != expected {
            exit_error_expected_but_found(expected.to_string(), nc.unwrap().to_string(), self.line, self.column, ERROR_SYNTAX);
        }
    }

    fn expect_next_specific(&mut self, expected: char) {
        self.expect_specific(self.next(), expected);
    }

    fn expect_specific_str(&mut self, expected: &str) {
        for c in expected.chars() {
            self.expect_specific(self.next(), c);
        }
    }

    /// Expect any of the given characters to be next in the file.
    /// Increments the line and column counters.
    /// 
    /// # Errors
    /// If EOF is reached, or the next character is not one of the expected ones, an error is returned and the program exits.
    fn expect_any_of(&mut self, nc: Option<char>, cs: Vec<char>) -> char {
        if nc.is_none() {
            exit_error_unexpected("EOF".to_string(), self.line, self.column, ERROR_SYNTAX);
        }
        let nc = nc.unwrap();
        if !cs.contains(&nc) {
            exit_error_expected_any_of_but_found(format!("{:?}", cs), nc, self.line, self.column, ERROR_SYNTAX);
        }
        nc
    }

    fn expect_next_any_of(&mut self, cs: Vec<char>) -> char {
        self.expect_any_of(self.next(), cs)
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
        }
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
    fn parse_include(&mut self, includes: &Vec<String>) {
        self.expect_specific_str("nclude");
        self.expect_next_specific('"');
        let path = self.parse_string();
        self.expect_any_of(self.next_skip_whitespace(false), vec!['\n', ';']);
        includes.push(path);
    }

    fn parse_location(&mut self) -> Result<WebXScope, String> {
        self.expect_specific_str("ocation");
        self.expect_specific(self.next_skip_whitespace(false), '{');
        self.parse_scope(false)
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
            match c.unwrap() {
                '}' => {
                    if is_global { exit_error_unexpected_char('}', self.line, self.column, ERROR_SYNTAX); }
                    else { break; }
                },
                '/' => self.parse_comment(),
                'i' => self.parse_include(&scope.includes),
                'l' => scope.scopes.push(self.parse_location()?),
                'm' => self.parse_model(),
                'h' => self.parse_handler(),
                'r' => self.parse_route(),
                't' => self.parse_type(),
                _ => exit_error_unexpected_char(c.unwrap(), self.line, self.column, ERROR_SYNTAX),
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
