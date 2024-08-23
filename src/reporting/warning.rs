use chrono::prelude::*;
use colored::*;

use crate::runner::WXMode;

fn warning_generic(mode: WXMode, message: String, warning_name: &str) {
    if mode.is_dev() && mode.debug_level().is_high() {
        eprintln!("{}: {}", warning_name.yellow(), message);
    }
}

pub fn warning(mode: WXMode, message: String) {
    let now = Local::now();
    let time = now.format("%d/%m %H:%M:%S");
    warning_generic(mode, message, format!("Warn (T{})", time).as_str());
}
