use std::cmp::Ordering;

use crate::{
    engine::runtime::WXRuntimeError,
    file::webx::{WXModule, WXRoute, WXScope, WXUrlPath, WXUrlPathSegment, WXROOT_PATH},
    reporting::error::{
        exit_error, format_info_field, DateTimeSpecifier, ERROR_DUPLICATE_ROUTE,
        ERROR_INVALID_ROUTE,
    },
};

// Keep every declaration, including duplicates in different location scopes.
pub type FlatRoutes = Vec<(WXRoute, WXUrlPath)>;

fn flatten_scopes(scope: &WXScope, prefix: &WXUrlPath, routes: &mut FlatRoutes) {
    for route in &scope.routes {
        routes.push((route.clone(), prefix.combine(&route.path)));
    }
    for child in &scope.scopes {
        flatten_scopes(child, &prefix.combine(&child.path), routes);
    }
}

pub fn extract_flat_routes(modules: &[WXModule]) -> FlatRoutes {
    let mut routes = Vec::new();
    for module in modules {
        flatten_scopes(&module.scope, &WXROOT_PATH, &mut routes);
    }
    routes.sort_by(|(a, ap), (b, bp)| {
        a.method
            .as_str()
            .cmp(b.method.as_str())
            .then_with(|| compare_specificity(ap, bp))
            .then_with(|| ap.to_string().cmp(&bp.to_string()))
            .then_with(|| a.info.path.inner.cmp(&b.info.path.inner))
            .then_with(|| a.info.line.cmp(&b.info.line))
    });
    routes
}

fn rank(segment: &WXUrlPathSegment) -> u8 {
    match segment {
        WXUrlPathSegment::Literal(_) => 2,
        WXUrlPathSegment::Parameter(_) => 1,
        WXUrlPathSegment::Regex(_, _) => 0,
    }
}

/// Most specific first: compare segments from left to right, then length.
/// Matching requires equal lengths; the length comparison only orders the table.
pub fn compare_specificity(a: &WXUrlPath, b: &WXUrlPath) -> Ordering {
    b.0.iter().map(rank).cmp(a.0.iter().map(rank))
}

fn same_shape(a: &WXUrlPath, b: &WXUrlPath) -> bool {
    a.segments() == b.segments()
        && a.0.iter().zip(&b.0).all(|(a, b)| match (a, b) {
            (WXUrlPathSegment::Literal(a), WXUrlPathSegment::Literal(b)) => a == b,
            (WXUrlPathSegment::Parameter(_), WXUrlPathSegment::Parameter(_)) => true,
            (WXUrlPathSegment::Regex(_, a), WXUrlPathSegment::Regex(_, b)) => a == b,
            _ => false,
        })
}

fn may_overlap(a: &WXUrlPath, b: &WXUrlPath) -> bool {
    a.segments() == b.segments()
        && a.0.iter().zip(&b.0).all(|(a, b)| match (a, b) {
            (WXUrlPathSegment::Literal(a), WXUrlPathSegment::Literal(b)) => a == b,
            (WXUrlPathSegment::Literal(text), WXUrlPathSegment::Regex(_, pattern))
            | (WXUrlPathSegment::Regex(_, pattern), WXUrlPathSegment::Literal(text)) => {
                pattern == "*"
                    || regex::Regex::new(&format!("^(?:{})$", pattern))
                        .is_ok_and(|re| re.is_match(text))
            }
            _ => true,
        })
}

pub fn extract_duplicate_routes(routes: &FlatRoutes) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (i, (a, ap)) in routes.iter().enumerate() {
        for (b, bp) in &routes[i + 1..] {
            if a.method == b.method && same_shape(ap, bp) {
                diagnostics.push(format!("Duplicate/ambiguous route {} {} at {} conflicts with {} at {} (parameter names and types do not distinguish routes)",
                    a.method, ap, format_info_field(&a.info), bp, format_info_field(&b.info)));
            }
        }
    }
    diagnostics
}

/// Warn about overlaps that are not a simple specific-route/fallback pair.
pub fn ambiguous_route_warnings(routes: &FlatRoutes) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (i, (a, ap)) in routes.iter().enumerate() {
        for (b, bp) in &routes[i + 1..] {
            let a_more = ap.0.iter().zip(&bp.0).any(|(a, b)| rank(a) > rank(b));
            let b_more = ap.0.iter().zip(&bp.0).any(|(a, b)| rank(b) > rank(a));
            if a.method == b.method
                && !same_shape(ap, bp)
                && may_overlap(ap, bp)
                && ((a_more && b_more) || compare_specificity(ap, bp) == Ordering::Equal)
            {
                diagnostics.push(format!("Ambiguous routes {} {} at {} and {} at {}; precedence is left-to-right literal > parameter > wildcard, then lexical path/source order",
                    a.method, ap, format_info_field(&a.info), bp, format_info_field(&b.info)));
            }
        }
    }
    diagnostics
}

pub fn analyze_duplicate_routes(modules: &[WXModule]) -> Result<FlatRoutes, WXRuntimeError> {
    let routes = extract_flat_routes(modules);
    let duplicates = extract_duplicate_routes(&routes);
    if !duplicates.is_empty() {
        return Err(WXRuntimeError {
            code: ERROR_DUPLICATE_ROUTE,
            message: duplicates.join("\n"),
        });
    }
    Ok(routes)
}

pub fn analyze_invalid_routes(modules: &[WXModule]) -> Result<(), WXRuntimeError> {
    let mut errors = Vec::new();
    for (route, path) in extract_flat_routes(modules) {
        // A body declaration is optional for write methods. Never unwrap a missing declaration.
        if matches!(
            route.method,
            hyper::Method::GET | hyper::Method::HEAD | hyper::Method::DELETE
        ) && route.body_format.is_some()
        {
            errors.push(format!(
                "{} {} cannot declare a request body ({})",
                route.method,
                path,
                format_info_field(&route.info)
            ));
        }
        for segment in &path.0 {
            if let WXUrlPathSegment::Regex(_, pattern) = segment {
                if pattern != "*" && regex::Regex::new(&format!("^(?:{})$", pattern)).is_err() {
                    errors.push(format!(
                        "Invalid route regex {:?} at {}",
                        pattern,
                        format_info_field(&route.info)
                    ));
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(WXRuntimeError {
            code: ERROR_INVALID_ROUTE,
            message: errors.join("\n"),
        })
    }
}

pub fn analyze_module_routes(modules: &[WXModule]) {
    if let Err(err) = verify_model_routes(modules) {
        exit_error(err.message, err.code, DateTimeSpecifier::None);
    }
}

pub fn verify_model_routes(modules: &[WXModule]) -> Result<FlatRoutes, WXRuntimeError> {
    let routes = analyze_duplicate_routes(modules)?;
    analyze_invalid_routes(modules)?;
    for message in ambiguous_route_warnings(&routes) {
        eprintln!("Route warning: {}", message);
    }
    Ok(routes)
}
