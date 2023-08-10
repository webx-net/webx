use std::{path::PathBuf, io::{BufReader, Read, Seek, SeekFrom}};
use crate::{file::webx::WebXFile, reporting::error::{exit_error, ERROR_PARSE_IO, ERROR_SYNTAX}};

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
    fn expect_specific(&mut self, c: char) {
        let nc = self.next();
        if nc.is_none() {
            exit_error(format!("Unexpected EOF, expected '{}' at line {}, column {}", c, self.line, self.column), ERROR_SYNTAX);
        } else if nc.unwrap() != c {
            exit_error(format!("Expected '{}' but found '{}' at line {}, column {}", c, nc.unwrap(), self.line, self.column), ERROR_SYNTAX);
        }
    }

    fn expect_specific_str(&mut self, s: &str) {
        for c in s.chars() {
            self.expect_specific(c);
        }
    }

    /// Expect any of the given characters to be next in the file.
    /// Increments the line and column counters.
    /// 
    /// # Errors
    /// If EOF is reached, or the next character is not one of the expected ones, an error is returned and the program exits.
    fn expect_any_of(&mut self, cs: Vec<char>) -> char {
        let nc = self.next();
        if nc.is_none() {
            exit_error(format!("Unexpected EOF, expected any of {:?} at line {}, column {}", cs, self.line, self.column), ERROR_SYNTAX);
        }
        let nc = nc.unwrap();
        if !cs.contains(&nc) {
            exit_error(format!("Expected any of {:?} but found '{}' at line {}, column {}", cs, nc.unwrap(), self.line, self.column), ERROR_SYNTAX);
        }
        nc
    }

    fn parse_comment(&mut self) {
        match self.expect_any_of(vec!['/', '*']) {
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

    fn parse_location_scope(&mut self) {
        self.expect_specific_str("ocation");
        loop {
            let c = self.next();
            if c.is_none() { break; }
            if c.unwrap() == '}' { break; }
        }
    }

    fn parse_scope(&mut self) {
        
    }

    fn parse_module(&mut self) -> Result<WebXFile, String> {
        let mut module = WebXFile {
            path: self.file.clone(),
            includes: vec![],
            scopes: vec![],
        };

        // Keywords: handler, include, location, module, { } and all HTTP methods.
        // Only expect a keyword at the start of a line, whitespace, or // comments.
        // Pass to dedicated parser function, otherwise error.

        loop {
            let c = self.next();
            if c.is_none() { break; }
            let c = c.unwrap();
            match c {
                ' ' | '\t' | '\n' => (), // Ignore whitespace.
                '/' => self.parse_comment(),
                '{' => self.parse_location_scope(),
                'i' => self.parse_include(),
                'l' => self.parse_location(),
                'm' => self.parse_module_keyword(),
                'h' => self.parse_handler(),
            }
        }

        Ok(module)
    }
}

pub fn parse_webx_file(file: &PathBuf) -> Result<WebXFile, String> {
    let file_contents = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
    let mut parser = WebXFileParser::new(file, &file_contents);
    Ok(parser.parse_module()?)
}
