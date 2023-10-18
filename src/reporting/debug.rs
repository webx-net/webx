use colored::Colorize;
use chrono::prelude::*;

use crate::runner::WXMode;

pub fn info(mode: WXMode, text: &str) {
    if mode.is_dev() && mode.debug_level().is_high() {
        let now = Local::now();
        let time = now.format("%d/%m %H:%M:%S");
        let prefix = format!("[INFO {}]", time);
        println!("{}: {}", prefix.bright_cyan(), text);
    }
}
