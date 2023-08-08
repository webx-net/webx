use std::path::Path;

pub fn run(dir: &Path, prod: bool) {
    println!("Running web server in {} mode", if prod { "production" } else { "development" });
    println!("Directory: {}", dir.display());
}
