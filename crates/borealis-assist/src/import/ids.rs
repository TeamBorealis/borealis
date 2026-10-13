//! Deterministic element ids: `ID_<PREFIX>_<TYPE>_<NAME>`.

use std::collections::HashSet;

use serde_json::Value;

/// Upper-case ASCII alphanumerics joined by single underscores.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_uppercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let t = out.trim_matches('_');
    if t.is_empty() {
        "UNNAMED".to_string()
    } else {
        t.to_string()
    }
}

/// The id for an element of `ty` called `name`, made unique against `used`.
pub fn make_id(prefix: &str, ty: &str, name: &str, used: &HashSet<String>) -> String {
    let base = format!("ID_{}_{}_{}", slug(prefix), slug(ty), slug(name));
    if !used.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}_{n}"))
        .find(|c| !used.contains(c))
        .unwrap_or(base)
}

/// Replace every element id of `content` with a deterministic one and fix references to the old
/// ids. Returns the mapping `(old, new)`.
pub fn reassign_ids(
    content: &mut Value,
    prefix: &str,
    used: &mut HashSet<String>,
) -> Vec<(String, String)> {
    let mut map = Vec::new();
    if let Some(els) = content.get("elements").and_then(Value::as_array) {
        for el in els {
            let s = |k: &str| {
                el.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            let old = s("id");
            let new = make_id(prefix, &s("type"), &s("name"), used);
            used.insert(new.clone());
            map.push((old, new));
        }
    }
    if map.is_empty() {
        return map;
    }
    let mut text = content.to_string();
    for (old, new) in &map {
        text = text.replace(&format!("\"{old}\""), &format!("\"{new}\""));
    }
    if let Ok(v) = serde_json::from_str(&text) {
        *content = v;
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_deterministic_and_unique() {
        let mut used = HashSet::new();
        let a = make_id("gloom", "Magic Item", "Ember's Ward!", &used);
        assert_eq!(a, "ID_GLOOM_MAGIC_ITEM_EMBER_S_WARD");
        used.insert(a.clone());
        assert_eq!(
            make_id("gloom", "Magic Item", "Ember's Ward!", &used),
            format!("{a}_2")
        );
    }
}
