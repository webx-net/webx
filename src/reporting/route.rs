use crate::file::webx::WXUrlPath;
use colored::*;
use hyper::Method;

pub fn print_route(method: &Method, path: &WXUrlPath) -> String {
    format!(
        "{} {}",
        method.to_string().bright_green(),
        path.to_string().bright_yellow(),
    )
}
