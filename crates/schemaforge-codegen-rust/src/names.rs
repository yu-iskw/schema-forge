//! Identifier sanitization, case conversion, and string escaping utilities.

use std::collections::HashSet;

// ── Case conversion ──────────────────────────────────────────────────────────

/// Convert `camelCase` or mixed-case to `snake_case`.
pub(crate) fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.extend(ch.to_lowercase());
    }
    result.replace(['-', ' '], "_")
}

/// Convert to `PascalCase`.
pub(crate) fn to_pascal_case(s: &str) -> String {
    s.split(['_', '-', ' '])
        .filter(|p| !p.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |c| {
                c.to_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

// ── Collision-free name allocation ───────────────────────────────────────────

/// Return `proposed` if it is not yet in `used`, otherwise append `_1`, `_2`,
/// … until a free name is found.  The chosen name is inserted into `used`.
pub(crate) fn unique_name(proposed: &str, used: &mut HashSet<String>) -> String {
    if used.insert(proposed.to_owned()) {
        return proposed.to_owned();
    }
    let mut i = 1usize;
    loop {
        let candidate = format!("{proposed}_{i}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        i += 1;
    }
}

// ── Identifier sanitization and string escaping ──────────────────────────────

/// Replace newlines, carriage-returns, and other ASCII control characters in
/// `s` with a single space so the result is safe to embed in a Rust line
/// comment (`//` or `///`) without prematurely terminating the comment or
/// injecting code on the next source line.
pub(crate) fn sanitize_for_comment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == '\n' || c == '\r' || c.is_ascii_control() {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// Return `true` when `s` is a Rust strict keyword or reserved word that
/// cannot appear as a bare identifier in source.
pub(crate) fn is_rust_keyword(s: &str) -> bool {
    matches!(
        s,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "union"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
    )
}

/// Escape a string for safe embedding inside a Rust string literal used in
/// `#[serde(rename = "...")]`.
///
/// Escapes `\`, `"`, newline, carriage-return, and tab so the resulting bytes
/// cannot break out of the attribute string literal.  Every other Unicode
/// scalar is passed through unchanged because Rust string literals accept
/// arbitrary Unicode.
pub(crate) fn escape_for_serde_rename(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// Strip every character that is not `[A-Za-z0-9_]` from a string that is
/// intended to become a Rust identifier.
///
/// If the first remaining character is an ASCII digit, a leading `_` is added
/// so the result is a valid identifier.  Returns an empty string when no safe
/// characters remain (callers must supply a fallback).
pub(crate) fn sanitize_identifier_chars(s: &str) -> String {
    let filtered: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if filtered.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{filtered}")
    } else {
        filtered
    }
}

/// Derive a safe `snake_case` Rust field identifier from a JSON property key.
///
/// Falls back to `field_{idx}` when no safe characters survive sanitization.
/// If the resulting identifier is a Rust keyword, it is wrapped as a raw
/// identifier (`r#keyword`) so it remains a valid field name.
pub(crate) fn sanitize_field_name(key: &str, idx: usize) -> String {
    let snake = to_snake_case(key);
    let safe = sanitize_identifier_chars(&snake);
    if safe.is_empty() {
        format!("field_{idx}")
    } else if is_rust_keyword(&safe) {
        format!("r#{safe}")
    } else {
        safe
    }
}

/// Derive a safe `PascalCase` Rust type identifier from a schema key.
///
/// Falls back to `Type{idx}` when no safe characters survive sanitization.
/// If the resulting identifier is a Rust keyword, a `_` suffix is appended
/// (raw-identifier syntax is not conventional for type names).
pub(crate) fn sanitize_type_name(key: &str, idx: usize) -> String {
    let pascal = to_pascal_case(key);
    let safe = sanitize_identifier_chars(&pascal);
    if safe.is_empty() {
        format!("Type{idx}")
    } else if is_rust_keyword(&safe) {
        format!("{safe}_")
    } else {
        safe
    }
}
