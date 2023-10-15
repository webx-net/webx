use colored::Colorize;

use crate::runner::WXMode;

pub fn info(mode: WXMode, text: &str) {
    if mode.is_dev() && mode.debug_level().is_high() {
        println!("{}: {}", "[INFO]".bright_cyan(), text);
    }
}
