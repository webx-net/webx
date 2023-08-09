use colored::*;

fn warning_generic(message: String, warning_name: &str) {
    eprintln!("{} {}", warning_name.yellow(), message);
}

pub fn warning(message: String) {
    warning_generic(message, "Warning");
}
