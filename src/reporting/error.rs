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
        ERROR_READ_WEBX_FILES => "Read".to_owned(),
        ERROR_PROJECT => "Project".to_owned(),
        ERROR_CIRCULAR_DEPENDENCY => "Circular Dependency".to_owned(),
        ERROR_DUPLICATE_ROUTE => "Duplicate Route".to_owned(),
        ERROR_INVALID_ROUTE => "Invalid Route".to_owned(),
        ERROR_HANDLER_CALL => "Handler Call".to_owned(),
        ERROR_PARSE_IO => "Parse IO".to_owned(),
        ERROR_SYNTAX => "Syntax".to_owned(),
        _ => format!("#{}", code),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateTimeSpecifier {
    Verbose,
    Short,
    None,
}

fn error_generic(message: String, error_name: &str) {
    eprintln!("{}: {}", error_name.red(), message);
}

fn error_generic_code(message: String, code: i32, date: DateTimeSpecifier) {
    let now = Local::now();
    if date == DateTimeSpecifier::None {
        error_generic(message, format!("{} Error", code_to_name(code)).as_str());
    } else {
        let time = match date {
            DateTimeSpecifier::Verbose => now.format("%d/%m %H:%M:%S"),
            DateTimeSpecifier::Short => now.format("%H:%M"),
            DateTimeSpecifier::None => unreachable!(),
        };
        error_generic(
            message,
            format!("{} Error (T{})", code_to_name(code), time).as_str(),
        );
    }
}

fn exit_error_generic_code(message: String, code: i32, date: DateTimeSpecifier) -> ! {
    error_generic_code(message, code, date);
    std::process::exit(code);
}

pub fn error_code(message: String, code: i32, date: DateTimeSpecifier) {
    error_generic_code(message, code, date);
}

pub fn exit_error(message: String, code: i32, date: DateTimeSpecifier) -> ! {
    exit_error_generic_code(message, code, date);
}

pub fn format_info_field(info: &WXInfoField) -> String {
    format!("{} line {}", info.path.module_name(), info.line)
        .bright_black()
        .to_string()
}

pub fn exit_error_hint(message: &str, hints: &[&str], code: i32, date: DateTimeSpecifier) -> ! {
    if hints.is_empty() {
        exit_error(message.into(), code, date);
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
    exit_error(format!("{}\n{}", message, hints), code, date)
}
