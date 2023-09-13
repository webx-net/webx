use colored::*;

// Error codes:
pub const ERROR_READ_WEBX_FILES: i32 = 1;
pub const ERROR_READ_PROJECT_CONFIG: i32 = 2;
pub const ERROR_CIRCULAR_DEPENDENCY: i32 = 3;
pub const ERROR_PARSE_IO: i32 = 4;
pub const ERROR_SYNTAX: i32 = 5;

pub fn code_to_name(code: i32) -> String {
    match code {
        ERROR_READ_WEBX_FILES => "READ_WEBX_FILES".to_owned(),
        ERROR_READ_PROJECT_CONFIG => "READ_PROJECT_CONFIG".to_owned(),
        ERROR_CIRCULAR_DEPENDENCY => "CIRCULAR_DEPENDENCY".to_owned(),
        ERROR_PARSE_IO => "PARSE_IO".to_owned(),
        ERROR_SYNTAX => "SYNTAX".to_owned(),
        _ => format!("UNKNOWN {}", code),
    }
}

fn error_generic(message: String, error_name: &str) {
    eprintln!("{}: {}", error_name.red(), message);
}

fn exit_error_generic(message: String, code: i32, error_name: &str) -> ! {
    error_generic(message, format!("{} ({})", error_name, code_to_name(code)).as_str());
    std::process::exit(code);
}

pub fn error(message: String) {
    error_generic(message, "Error");
}

pub fn exit_error(message: String, code: i32) -> ! {
    exit_error_generic(message, code, "Error");
}

pub fn exit_error_unexpected(what: String, context: &str, line: usize, column: usize, code: i32) -> ! {
    exit_error(format!("Unexpected {} while {} at line {}, column {}", what, context, line, column), code);
}

pub fn exit_error_expected_but_found(expected: String, found: String, context: &str, line: usize, column: usize, code: i32) -> ! {
    exit_error(format!("Expected {} but found '{}' while {} at line {}, column {}", expected, found, context, line, column), code);
}

pub fn exit_error_expected_any_of_but_found(expected: String, found: char, context: &str, line: usize, column: usize, code: i32) -> ! {
    exit_error(format!("Expected any of {} but found '{}' while {} at line {}, column {}", expected, found, context, line, column), code);
}

pub fn exit_error_unexpected_char(what: char, context: &str, line: usize, column: usize, code: i32) -> ! {
    exit_error_unexpected(format!("character '{}'", what), context, line, column, code);
}