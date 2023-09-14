use colored::*;

use std::collections::HashMap;

use crate::{file::webx::{WXModule, WXScope, WXUrlPath, WXROOT_PATH, WXRouteMethod, WXInfoField}, reporting::error::{exit_error, ERROR_DUPLICATE_ROUTE}};

type FlatRoutes = HashMap<(WXRouteMethod, WXUrlPath), Vec<WXInfoField>>;

fn extract_flat_routes(modules: &Vec<WXModule>) -> FlatRoutes {
    let mut routes = HashMap::new();
    fn flatten_scopes(module_name: String, scope: &WXScope, path_prefix: WXUrlPath, routes: &mut FlatRoutes) {
        for route in scope.routes.iter() {
            let flat_path = path_prefix.combine(&route.path);
            let route_key = (route.method.clone(), flat_path);
            if routes.contains_key(&route_key) {
                routes.get_mut(&route_key).unwrap().push(route.info.clone());
            } else {
                routes.insert(route_key, vec![route.info.clone()]);
            }
        }
        for sub_scope in scope.scopes.iter() {
            let sub_scope_path = path_prefix.combine(&sub_scope.path);
            flatten_scopes(module_name.clone(), sub_scope, sub_scope_path, routes);
        }
    }
    for module in modules.iter() {
        flatten_scopes(module.path.module_name(), &module.scope, WXROOT_PATH, &mut routes);
    }
    routes
}

fn extract_duplicate_routes(routes: &FlatRoutes) -> Vec<String> {
    routes
        .iter()
        .filter(|(_, modules)| modules.len() > 1)
        .map(|((method, path), modules)| {
            let locations = modules
                .iter()
                .map(|info|
                    format!("{} line {}", info.path.module_name(), info.line)
                    .bright_black().to_string())
                .collect::<Vec<_>>();
            format!(
                "Route {} {} is defined in modules:\n    - {}",
                method.to_string().green(),
                path.to_string().yellow(),
                locations.join("\n    - ")
            )
        })
        .collect()
}

fn analyse_duplicate_routes(modules: &Vec<WXModule>) {
    let routes = extract_flat_routes(modules);
    let duplicate_routes = extract_duplicate_routes(&routes);
    if !duplicate_routes.is_empty() {
        exit_error(
            format!(
                "Duplicate routes detected:\n  - {}",
                duplicate_routes.join("\n  - ")
            ),
            ERROR_DUPLICATE_ROUTE,
        );
    }
}

fn analyse_invalid_routes(modules: &Vec<WXModule>) {
}

pub fn analyse_module_routes(modules: &Vec<WXModule>) {
    analyse_duplicate_routes(modules);
    analyse_invalid_routes(modules);
}

// Route verification, check for:
//       - invalid route combinations (e.g. GET + body)
//       - return type for each route (HTML, JSON, or unknown)
