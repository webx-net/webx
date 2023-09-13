use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    file::webx::WXFile,
    reporting::{
        error::{exit_error, ERROR_CIRCULAR_DEPENDENCY},
        warning::warning,
    },
};

/// The configuration for a WebX project.
///
/// ## Example
/// ```json
/// {
///     "name": "My WebX Project",
///     "version": "1.0.0",
///     "description": "An example WebX project.",
///     "port": 8080,
///     "host": "localhost",
///     "src": "./webx/",
///     "database": {
///         "type": "postgresql",
///         "host": "localhost",
///         "port": 5432,
///         "username": "user",
///         "password": "password",
///         "databaseName": "webx_db"
///     },
///     "logLevel": "debug",
///     "cors": {
///         "allowOrigin": "*"
///     },
///     "rateLimit": {
///         "windowMs": 60000,
///         "maxRequests": 100
///     },
///     "migrationsPath": "./migrations/",
///     "cache": {
///         "strategy": "memory",
///         "duration": "10m"
///     }
/// }
/// ```
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub port: u16,
    pub host: String,
    pub src: PathBuf,
    pub log_level: Option<String>,
    pub migrations_path: Option<PathBuf>,
    pub cors: Option<CorsConfig>,
    pub rate_limit: Option<RateLimitConfig>,
    pub database: Option<DatabaseConfig>,
    pub cache: Option<CacheConfig>,
}

/// The configuration for the CORS middleware.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorsConfig {
    pub allow_origin: String,
}

/// The configuration for the rate limit middleware.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitConfig {
    pub window_ms: u64,
    pub max_requests: u64,
}

/// The configuration for the database.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseConfig {
    pub database_type: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database_name: String,
}

/// The configuration for the cache.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheConfig {
    pub strategy: String,
    pub duration: String,
}

/// Parse the project configuration from a given filepath.
///
/// ## Arguments
/// - `config` - The path to the project configuration file.
///
/// ## Returns
/// The project configuration.
pub fn load_project_config(config_file: &PathBuf) -> ProjectConfig {
    let txt = fs::read_to_string(config_file).expect("Failed to open project configuration.");
    let config: ProjectConfig =
        serde_json::from_str(&txt).expect("Failed to parse project configuration.");
    config
}

/// Recursively find all `.webx` files in a given directory.
///
/// ## Arguments
/// - `src` - The path to the source directory.
///
/// ## Returns
/// A vector of canonical paths to all .webx files in the project's source directory.
///
/// ## Errors
/// If the source directory does not exist, an error is returned.
pub fn locate_webx_files(src: &Path) -> Result<Vec<PathBuf>, String> {
    let src = src.to_path_buf();
    if !src.exists() {
        return Err(format!("The directory '{}' does not exist.", src.display()));
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(src).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            // Recursively find all .webx files in the directory.
            files.append(&mut locate_webx_files(&path).unwrap());
        } else if path.is_file() {
            files.push(path.canonicalize().map_err(|e| e.to_string())?);
        } else {
            panic!(
                "The path '{}' is neither a file nor a directory.",
                path.display()
            );
        }
    }
    Ok(files)
}

type DependencyTree = HashMap<PathBuf, Vec<PathBuf>>;

pub fn detect_circular_dependencies(tree: &DependencyTree) -> Vec<PathBuf> {
    let mut circular_dependencies = Vec::new();
    for (_, dependents) in tree {
        for dependent in dependents {
            if tree.contains_key(dependent) {
                circular_dependencies.push(dependent.clone());
            }
        }
    }
    circular_dependencies
}

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
pub fn construct_dependency_tree(files: &Vec<WXFile>) -> DependencyTree {
    let mut tree = DependencyTree::new();
    for file in files.iter() {
        // Insert dependencies into the tree as keys and the file path as the value.
        for dependency in file.module_scope.includes.iter() {
            let dependency_target = file.path.join(dependency);
            tree.entry(dependency_target)
                .or_insert(Vec::new())
                .push(file.path.clone());
        }
    }
    tree
}

/// Create a new WebX project in the given directory.
///
/// ## Arguments
/// - `root_dir` - The path to the root directory of the project.
/// - `override_existing` - Whether or not to override an existing project.
///
/// ## File Structure
/// The following files are added to the root directory:
/// ```text
/// root/
/// |  webx.config.json
/// |  webx/
///    |  index.webx
/// ```
/// The `webx.config.json` file contains the default configuration for the project.
/// The `webx/` directory contains all of the WebX source files.
/// The `index.webx` file contains some default example code.
///
/// ## Warning
/// If a `webx.config.json` file already exists in the root directory,
/// and `override_existing` is set to `false`, then a warning is printed and
/// the function returns.
pub fn create_new_project(name: String, root_dir: &PathBuf, override_existing: bool) {
    let root_dir = root_dir.to_path_buf().join(&name);
    let config_file = root_dir.join("webx.config.json");
    let src_dir = root_dir.join("webx");
    let index_file = src_dir.join("index.webx");

    if config_file.exists() && !override_existing {
        warning("A WebX project already exists in this directory.".into());
        return;
    }

    let default_config = ProjectConfig {
        name: format!("My {} WebX Project", name),
        version: "1.0.0".to_string(),
        description: Some("An example WebX project.".to_string()),
        port: 8080,
        host: "localhost".to_string(),
        src: PathBuf::from("./webx/"),
        log_level: None,
        migrations_path: None,
        cors: Some(CorsConfig {
            allow_origin: "*".to_string(),
        }),
        rate_limit: None,
        database: None,
        cache: None,
    };

    const DEFAULT_INDEX_FILE_CONTENTS: &str = r#"// This is an example WebX todo app project.
global {
    // Global in-memory database of todos for this example.
    const todos = [];
}

model Todo {
    title: String,
    completed: Boolean,
    createdAt: Date,
}

handler renderTodo(todo: Todo) (<h1>
    <input type="checkbox" checked={todo.completed} />
    {todo.title} - {getTimeDiff(todo.createdAt)}
</h1>)

handler renderAllTodos(todos: Todo[])
(<ul class="todos">{todos.map(renderTodo)}</ul>)

handler auth(user_id: Int) {
    if (user_id === 0) return error("You are not logged in.");
}

get about/ (<div>
    <h1>About</h1>
    <p>This is an example WebX project.</p>
</div>)

location /todo {
    // Display the global list of todos as HTML.
    get /(user_id: Int)/list -> auth(user_id), renderAllTodos(todos)

    // Add a new todo to the list with the given title.
    // { title: "My Todo" }
    // returns HTML
    post (user_id: Int)/add json(title: String) -> auth(user_id) {
        const newTodo = { title, completed: false };
        todos.push(newTodo);
    } -> renderTodo(newTodo)

    // Toggle the completed status of the todo with the given id.
    // { id: 0 }
    // returns HTML
    post (user_id: Int)/toggle json(todo_id: Int) -> auth(user_id) {
        const todo = todos.find(t => t.id === todo_id);
        if (todo) {
            todo.completed = !todo.completed;
        } else {
            return error("Todo not found.");
        }
    } -> renderTodo(todo)
}
"#;

    fs::create_dir_all(&src_dir).expect("Failed to create source directory.");
    fs::write(&index_file, DEFAULT_INDEX_FILE_CONTENTS).expect("Failed to create index file.");
    fs::write(
        &config_file,
        serde_json::to_string_pretty(&default_config).unwrap(),
    )
    .expect("Failed to create config file.");
}
