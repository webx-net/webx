use deno_core::{error::AnyError, extension, include_js_files, op2, Extension};

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
        message: format!("static: failed to read file '{}'", relative_path.to_raw()),
        code: ERROR_HANDLER_CALL,
    })
}

/// Try to call a native function by name. \
/// TODO: Figure out if this should be replaced with a JS extension.
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

// #[op]
// async fn op_webx_static(relative_path: String) -> Result<String, AnyError> {
//     let file = std::fs::read_to_string(relative_path).await?;
//     Ok(file)
// }

pub fn init() -> Extension {
    Extension {
        name: "webx stdlib",
        ops: vec![].into(), //  vec![op_webx_static::decl()],
        esm_files: include_js_files!(stdlib "src/engine/stdlib.js",)
            .to_vec()
            .into(),
        ..Default::default()
    }
}

pub const JAVASCRIPT: &str = include_str!("./stdlib.js");
