use colored::*;

use crate::runner::WXMode;

fn warning_generic(mode: WXMode, message: String, warning_name: &str) {
    if mode.is_dev() && mode.debug_level().is_medium() {
        eprintln!("{}: {}", warning_name.yellow(), message);
    }
}

pub fn warning(mode: WXMode, message: String) {
    warning_generic(mode, message, "[Warning]");
}
