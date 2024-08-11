use std::{collections::HashMap, path::PathBuf};

use crate::{
    file::webx::WXModule,
    reporting::error::{exit_error, DateTimeSpecifier, ERROR_CIRCULAR_DEPENDENCY},
};

type DependencyTree = HashMap<PathBuf, Vec<PathBuf>>;

/// Construct a dependency tree from a list of WebX files.
/// The tree is a hashmap where the keys are the dependencies and the values are the files that
/// depend on them.
/// If a circular dependency is detected, an error is returned.
///
/// ## Arguments
/// - `files` - The list of WebX files.
///
/// ## Returns
/// The dependency tree.
fn construct_dependency_tree(files: &[WXModule]) -> DependencyTree {
    let mut tree = DependencyTree::new();
    for file in files.iter() {
        // Insert dependencies into the tree as keys and the file path as the value.
        for dependency in file.scope.includes.iter() {
            let dependency_target = file.path.inner.join(dependency);
            tree.entry(dependency_target)
                .or_default()
                .push(file.path.inner.clone());
        }
    }
    tree
}

fn detect_circular_dependencies(tree: &DependencyTree) -> Vec<PathBuf> {
    let mut circular_dependencies = Vec::new();
    for dependents in tree.values() {
        for dependent in dependents {
            if tree.contains_key(dependent) {
                circular_dependencies.push(dependent.clone());
            }
        }
    }
    circular_dependencies
}

fn analyse_circle_dependencies(modules: &[WXModule]) {
    let dependency_tree = construct_dependency_tree(modules);
    let circular_dependencies = detect_circular_dependencies(&dependency_tree);
    if !circular_dependencies.is_empty() {
        exit_error(
            format!(
                "Circular dependencies detected:\n{:?}",
                circular_dependencies
            ),
            ERROR_CIRCULAR_DEPENDENCY,
            DateTimeSpecifier::None,
        );
    }
}

/// Analyse the dependencies of a list of WebX modules.
/// If a circular dependency is detected, an error is reported and the program exits.
pub fn analyse_module_deps(modules: &[WXModule]) {
    analyse_circle_dependencies(modules);
}
