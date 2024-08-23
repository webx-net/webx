use deno_core::{
    include_js_files,
    v8::{self, Global, Local, Value},
    Extension,
};

use crate::reporting::error::ERROR_HANDLER_CALL;

use super::runtime::{WXRuntimeError, WXRuntimeInfo};

/// Serve static content from the filesystem.
///
/// # Arguments
/// - `path`: The path to the file to serve relative to the project root.
fn webx_static(
    relative_path: &Local<'_, Value>,
    info: &WXRuntimeInfo,
) -> Result<Global<Value>, WXRuntimeError> {
    let mut isolate = v8::Isolate::new(Default::default());
    let mut scope = v8::HandleScope::new(&mut isolate);
    // Read the file from the filesystem.
    if let Ok(path) = Local::<'_, v8::String>::try_from(*relative_path) {
        let path = path.to_rust_string_lossy(&mut scope);
        let file = std::fs::read(info.project_root.join(path.clone()));
        if let Ok(file) = file {
            let content = String::from_utf8(file).unwrap();
            let local: Local<'_, v8::Value> = v8::String::new(&mut scope, &content).unwrap().into();
            return Ok(Global::new(&mut scope, local));
        } else {
            return Err(WXRuntimeError {
                message: format!("static: failed to read file '{}'", path),
                code: ERROR_HANDLER_CALL,
            });
        }
    }
    Err(WXRuntimeError {
        message: format!("static: failed to read file '{:?}'", relative_path),
        code: ERROR_HANDLER_CALL,
    })
}

/// Try to call a native function by name. \
/// TODO: Figure out if this should be replaced with a JS extension.
pub fn try_call(
    name: &str,
    args: &[Local<'_, Value>],
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
        "static" => assert_args(1).and_then(|_| webx_static(&args[0], info)),
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
