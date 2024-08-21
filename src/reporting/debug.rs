use chrono::prelude::*;
use colored::Colorize;

use crate::runner::WXMode;

pub fn info(mode: WXMode, text: &str) {
    if mode.is_dev() && mode.debug_level().is_medium() {
        let now = Local::now();
        let time = now.format("%d/%m %H:%M:%S");
        let prefix = format!("[Info {}]", time);
        println!("{}: {}", prefix.bright_cyan(), text);
    }
}
