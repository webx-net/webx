#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::runner::{run, WXMode};

    #[test]
    fn test_example_todo() {
        run(&Path::new("examples/todo"), WXMode::Dev);
    }
}
