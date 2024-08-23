use deno_core::{
    v8::{self, Global, Local, Value},
    JsRuntime,
};

use crate::reporting::error::ERROR_HANDLER_CALL;

use super::runtime::{WXRuntimeError, WXRuntimeInfo};

/// Serve static content from the filesystem.
///
/// # Arguments
/// - `path`: The path to the file to serve relative to the project root.
fn webx_static(
    global_relative_path: &Global<Value>,
    rt: &mut JsRuntime,
    info: &WXRuntimeInfo,
) -> Result<Global<Value>, WXRuntimeError> {
    let scope = &mut rt.handle_scope();
    // Read the file from the filesystem.
    let local_relative_path = Local::new(scope, global_relative_path);
    if let Ok(path) = Local::<'_, v8::String>::try_from(local_relative_path) {
        let path = path.to_rust_string_lossy(scope);
        let file = std::fs::read(info.project_root.join(path.clone()));
        if let Ok(file) = file {
            let content = String::from_utf8(file).unwrap();
            let local: Local<'_, v8::Value> = v8::String::new(scope, &content).unwrap().into();
            return Ok(Global::new(scope, local));
        } else {
            return Err(WXRuntimeError {
                message: format!("static: failed to read file '{}'", path),
                code: ERROR_HANDLER_CALL,
            });
        }
    }
    Err(WXRuntimeError {
        message: format!("static: failed to read file '{:?}'", global_relative_path),
        code: ERROR_HANDLER_CALL,
    })
}

/// Try to call a native function by name. \
/// TODO: Figure out if this should be replaced with a JS extension.
pub fn try_call(
    name: &str,
    args: &[Global<Value>],
    rt: &mut JsRuntime,
    info: &WXRuntimeInfo,
) -> Option<Result<Global<Value>, WXRuntimeError>> {
    let assert_args = |n: usize| {
        if args.len() != n {
            return Err(WXRuntimeError {
                message: format!("{}: expected {} arguments, got {}", name, n, args.len()),
                code: ERROR_HANDLER_CALL,
            });
        }
        Ok(())
    };

    Some(match name {
        "static" => assert_args(1).and_then(|_| webx_static(&args[0], rt, info)),
        _ => return None,
    })
}

// #[op]
// async fn op_webx_static(relative_path: String) -> Result<String, AnyError> {
//     let file = std::fs::read_to_string(relative_path).await?;
//     Ok(file)
// }

// pub fn init() -> Extension {
//     Extension {
//         name: "webx stdlib",
//         ops: vec![].into(), //  vec![op_webx_static::decl()],
//         esm_files: include_js_files!(stdlib "src/engine/stdlib.js",)
//             .to_vec()
//             .into(),
//         ..Default::default()
//     }
// }

pub const JAVASCRIPT: &str = include_str!("./stdlib.js");
