//! Core vocabulary keyword processing (`$ref`, `$id`, `$schema`, `$defs`).

use serde_json::{Map, Value};

use crate::{ValidationContext, ValidationOutput};

/// Apply core vocabulary keywords from `obj` to `instance`.
pub(crate) fn apply(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    apply_ref(obj, instance, path, ctx, out);
}

fn apply_ref(
    obj: &Map<String, Value>,
    instance: &Value,
    path: &str,
    ctx: &ValidationContext<'_>,
    out: &mut ValidationOutput,
) {
    let Some(Value::String(ref_uri)) = obj.get("$ref") else {
        return;
    };
    let schema = resolve_ref(ref_uri, ctx);
    let result = crate::validate_schema(&schema, instance, path, ctx);
    out.merge(result);
}

fn resolve_ref(ref_uri: &str, ctx: &ValidationContext<'_>) -> Value {
    if ref_uri.starts_with('#') {
        return Value::Bool(true);
    }
    let key = if ref_uri.starts_with("http://") || ref_uri.starts_with("https://") {
        ref_uri.to_owned()
    } else {
        format!("{}{}", ctx.base_uri, ref_uri)
    };
    ctx.registry.get(&key).cloned().unwrap_or(Value::Bool(true))
}
