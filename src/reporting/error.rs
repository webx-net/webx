use chrono::prelude::*;
use colored::*;

use crate::file::webx::WXInfoField;

// Error codes:
pub const ERROR_READ_WEBX_FILES: i32 = 1;
pub const ERROR_PROJECT: i32 = 2;
pub const ERROR_CIRCULAR_DEPENDENCY: i32 = 3;
pub const ERROR_PARSE_IO: i32 = 4;
pub const ERROR_SYNTAX: i32 = 5;
pub const ERROR_DUPLICATE_ROUTE: i32 = 6;
pub const ERROR_INVALID_ROUTE: i32 = 7;
pub const ERROR_HANDLER_CALL: i32 = 8;

pub fn code_to_name(code: i32) -> String {
    match code {
        ERROR_READ_WEBX_FILES => "READ_WEBX_FILES".to_owned(),
        ERROR_PROJECT => "PROJECT".to_owned(),
        ERROR_CIRCULAR_DEPENDENCY => "CIRCULAR_DEPENDENCY".to_owned(),
        ERROR_DUPLICATE_ROUTE => "DUPLICATE_ROUTE".to_owned(),
        ERROR_INVALID_ROUTE => "INVALID_ROUTE".to_owned(),
        ERROR_HANDLER_CALL => "HANDLER_CALL".to_owned(),
        ERROR_PARSE_IO => "PARSE_IO".to_owned(),
        ERROR_SYNTAX => "SYNTAX".to_owned(),
        _ => format!("UNKNOWN {}", code),
    }
}

fn error_generic(message: String, error_name: &str) {
    eprintln!("{}: {}", error_name.red(), message);
}

fn error_generic_code(message: String, code: i32, error_name: &str) {
    let now = Local::now();
    let time = now.format("%d/%m %H:%M:%S");
    error_generic(
        message,
        format!("[{} {} ({})]", error_name, time, code_to_name(code)).as_str(),
    );
}

fn exit_error_generic_code(message: String, code: i32, error_name: &str) -> ! {
    error_generic_code(message, code, error_name);
    std::process::exit(code);
}

pub fn error(message: String) {
    let now = Local::now();
    let time = now.format("%d/%m %H:%M:%S");
    error_generic(message, format!("[Error {}]", time).as_str());
}

pub fn error_code(message: String, code: i32) {
    error_generic_code(message, code, "Error");
}

pub fn exit_error(message: String, code: i32) -> ! {
    exit_error_generic_code(message, code, "Error");
}

pub fn exit_error_unexpected(
    what: String,
    context: &str,
    line: usize,
    column: usize,
    code: i32,
) -> ! {
    exit_error(
        format!(
            "Unexpected {} while {} at line {}, column {}",
            what, context, line, column
        ),
        code,
    );
}

pub fn exit_error_expected_but_found(
    expected: String,
    found: String,
    context: &str,
    line: usize,
    column: usize,
    code: i32,
) -> ! {
    exit_error(
        format!(
            "Expected {} but found '{}' while {} at line {}, column {}",
            expected, found, context, line, column
        ),
        code,
    );
}

pub fn exit_error_expected_any_of_but_found(
    expected: String,
    found: char,
    context: &str,
    line: usize,
    column: usize,
    code: i32,
) -> ! {
    exit_error(
        format!(
            "Expected any of {} but found '{}' while {} at line {}, column {}",
            expected, found, context, line, column
        ),
        code,
    );
}

pub fn exit_error_unexpected_char(
    what: char,
    context: &str,
    line: usize,
    column: usize,
    code: i32,
) -> ! {
    exit_error_unexpected(format!("character '{}'", what), context, line, column, code);
}

pub fn format_info_field(info: &WXInfoField) -> String {
    format!("{} line {}", info.path.module_name(), info.line)
        .bright_black()
        .to_string()
}

pub fn exit_error_hint(message: &str, hints: &[&str], code: i32) -> ! {
    if hints.is_empty() {
        exit_error(message.into(), code);
    }
    let hints = if hints.len() > 1 {
        const HINT_SEP: &str = "\n - ";
        format!(
            "{}: {}{}",
            "Hints".bright_yellow(),
            HINT_SEP,
            hints.join(HINT_SEP)
        )
    } else {
        format!("{}: {}", "Hint".bright_yellow(), hints[0])
    };
    exit_error(format!("{}\n{}", message, hints), code)
}
