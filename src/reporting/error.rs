use colored::*;

// Error codes:
pub static ERROR_READ_WEBX_FILES: i32 = 1;
pub static ERROR_READ_PROJECT_CONFIG: i32 = 2;
pub static ERROR_CIRCULAR_DEPENDENCY: i32 = 3;
pub static ERROR_PARSE_IO: i32 = 4;
pub static ERROR_SYNTAX: i32 = 5;

fn error_generic(message: String, error_name: &str) {
    eprintln!("{}: {}", error_name.red(), message);
}

fn exit_error_generic(message: String, code: i32, error_name: &str) -> ! {
    error_generic(message, format!("{} ({})", error_name, code).as_str());
    std::process::exit(code);
}

pub fn error(message: String) {
    error_generic(message, "Error");
}

pub fn exit_error(message: String, code: i32) -> ! {
    exit_error_generic(message, code, "Error");
}

pub fn exit_error_unexpected(what: String, line: usize, column: usize, code: i32) -> ! {
    exit_error(format!("Unexpected {} at line {}, column {}", what, line, column), code);
}

pub fn exit_error_expected_but_found(expected: String, found: String, line: usize, column: usize, code: i32) -> ! {
    exit_error(format!("Expected {} but found '{}' at line {}, column {}", expected, found, line, column), code);
}

pub fn exit_error_expected_any_of_but_found(expected: String, found: char, line: usize, column: usize, code: i32) -> ! {
    exit_error(format!("Expected any of {} but found '{}' at line {}, column {}", expected, found, line, column), code);
}

pub fn exit_error_unexpected_char(what: char, line: usize, column: usize, code: i32) -> ! {
    exit_error_unexpected(format!("character '{}'", what), line, column, code);
}