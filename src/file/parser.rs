use std::{path::PathBuf, io::BufReader};
use crate::file::webx::WebXFile;

pub fn parse_webx_file(file: &PathBuf) -> Result<WebXFile, String> {
    let module = WebXFile {
        path: file.clone(),
        includes: vec![],
        scopes: vec![],
    };
    let file_contents = std::fs::read_to_string(file).map_err(|e| e.to_string())?;

    let reader = BufReader::new(file_contents.as_bytes());

    Ok(module)
}
