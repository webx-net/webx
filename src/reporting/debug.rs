use colored::Colorize;

use crate::runner::WXMode;

pub fn info(mode: WXMode, text: &str) {
    if mode == WXMode::Dev {
        println!("{}: {}", "[INFO]".bright_cyan(), text);
    }
}
