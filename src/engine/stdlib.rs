use crate::reporting::error::ERROR_HANDLER_CALL;

use super::runtime::{WXRTValue, WXRuntimeError, WXRuntimeInfo};

/// Serve static content from the filesystem.
///
/// # Arguments
/// - `path`: The path to the file to serve relative to the project root.
fn webx_static(
    relative_path: &WXRTValue,
    info: &WXRuntimeInfo,
) -> Result<WXRTValue, WXRuntimeError> {
    // Read the file from the filesystem.
    if let WXRTValue::String(path) = relative_path {
        let file = std::fs::read(info.project_root.join(path));
        if let Ok(file) = file {
            return Ok(WXRTValue::String(String::from_utf8(file).unwrap()));
        } else {
            return Err(WXRuntimeError {
                message: format!("static: failed to read file '{}'", path),
                code: ERROR_HANDLER_CALL,
            });
        }
    }
    Err(WXRuntimeError {
        message: format!("static: failed to read file '{}'", relative_path.to_js()),
        code: ERROR_HANDLER_CALL,
    })
}

pub fn try_call(
    name: &str,
    args: &[WXRTValue],
    info: &WXRuntimeInfo,
) -> Option<Result<WXRTValue, WXRuntimeError>> {
    let assert_args = |n: usize| {
        if args.len() != n {
            return Some(WXRuntimeError {
                message: format!("{}: expected {} arguments, got {}", name, n, args.len()),
                code: ERROR_HANDLER_CALL,
            });
        }
        None
    };

    match name {
        "static" => Some(match assert_args(1) {
            None => webx_static(&args[0], info),
            Some(err) => Err(err),
        }),
        _ => None,
    }
}
