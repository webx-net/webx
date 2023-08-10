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
