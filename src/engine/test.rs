#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::engine::runner::run;

    #[test]
    fn test_example_todo() {
        run(&Path::new("examples/todo"), false);
    }
}
