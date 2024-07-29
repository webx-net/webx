use colored::*;

use std::collections::HashMap;

use crate::{
    file::webx::{WXInfoField, WXModule, WXRoute, WXScope, WXUrlPath, WXROOT_PATH},
    reporting::error::{exit_error, format_info_field, ERROR_DUPLICATE_ROUTE, ERROR_INVALID_ROUTE},
};

type FlatRoutes = HashMap<(WXRoute, WXUrlPath), Vec<WXInfoField>>;

fn flatten_scopes(
    module_name: String,
    scope: &WXScope,
    path_prefix: WXUrlPath,
    routes: &mut FlatRoutes,
) {
    for route in scope.routes.iter() {
        let flat_path = path_prefix.combine(&route.path);
        let route_key = (route.clone(), flat_path);
        if let std::collections::hash_map::Entry::Vacant(e) = routes.entry(route_key.clone()) {
            e.insert(vec![route.info.clone()]);
        } else {
            routes.get_mut(&route_key).unwrap().push(route.info.clone());
        }
    }
    for sub_scope in scope.scopes.iter() {
        let sub_scope_path = path_prefix.combine(&sub_scope.path);
        flatten_scopes(module_name.clone(), sub_scope, sub_scope_path, routes);
    }
}

pub fn extract_flat_routes(modules: &[WXModule]) -> FlatRoutes {
    let mut routes = HashMap::new();
    for module in modules.iter() {
        flatten_scopes(
            module.path.module_name(),
            &module.scope,
            WXROOT_PATH,
            &mut routes,
        );
    }
    routes
}

pub fn extract_duplicate_routes(routes: &FlatRoutes) -> Vec<String> {
    routes
        .iter()
        .filter(|(_, modules)| modules.len() > 1)
        .map(|((route, path), modules)| {
            let locations = modules.iter().map(format_info_field).collect::<Vec<_>>();
            format!(
                "Route {} {} is defined in modules:\n    - {}",
                route.method.to_string().green(),
                path.to_string().yellow(),
                locations.join("\n    - ")
            )
        })
        .collect()
}

pub fn analyse_duplicate_routes(modules: &[WXModule]) -> Result<FlatRoutes, (String, i32)> {
    let routes = extract_flat_routes(modules);
    let duplicate_routes = extract_duplicate_routes(&routes);
    if !duplicate_routes.is_empty() {
        return Err((
            format!(
                "Duplicate routes detected:\n  - {}",
                duplicate_routes.join("\n  - ")
            ),
            ERROR_DUPLICATE_ROUTE,
        ));
    }
    Ok(routes)
}

fn extract_invalid_routes(routes: &FlatRoutes) -> Vec<String> {
    routes
        .iter()
        .filter(|((route, _), _)| match route.method {
            hyper::Method::GET | hyper::Method::DELETE => route.body_format.is_some(),
            hyper::Method::POST | hyper::Method::PUT => route.body_format.is_none(),
            _ => false,
        })
        .map(|((route, path), info)| {
            format!(
                "Route {} {} specify {}, but is not a POST or PUT endpoint. {}",
                route.method.to_string().green(),
                path.to_string().yellow(),
                route.body_format.as_ref().unwrap().to_string().red(),
                format_info_field(info.first().unwrap()),
            )
        })
        .collect()
}

/// Analyse the implementation of routes in a list of WebX modules.
/// If an invalid route is detected, an error is reported and the program exits.
/// Invalid routes include:
/// - bad combinations of route methods and request body format types (e.g. GET + body)
pub fn analyse_invalid_routes(modules: &[WXModule]) -> Result<(), (String, i32)> {
    let routes = extract_flat_routes(modules);
    let invalid_routes = extract_invalid_routes(&routes);
    if !invalid_routes.is_empty() {
        return Err((
            format!(
                "Invalid routes detected:\n  - {}",
                invalid_routes.join("\n  - ")
            ),
            ERROR_INVALID_ROUTE,
        ));
    }
    Ok(())
}

fn exit_on_err<T>(result: Result<T, (String, i32)>) {
    if let Err((message, code)) = result {
        exit_error(message, code, false);
    }
}

pub fn analyse_module_routes(modules: &[WXModule]) {
    exit_on_err(analyse_duplicate_routes(modules));
    exit_on_err(analyse_invalid_routes(modules));
}

pub fn verify_model_routes(modules: &[WXModule]) -> Result<FlatRoutes, (String, i32)> {
    let routes = analyse_duplicate_routes(modules)?;
    analyse_invalid_routes(modules)?;
    Ok(routes)
}
