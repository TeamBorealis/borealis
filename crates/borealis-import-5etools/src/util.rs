//! Small string helpers shared by the converters.

use serde_json::Value;

/// FNV-1a 64 bit hash (stable across platforms and releases).
#[must_use]
pub fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn fold_char(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => "A",
        'ç' | 'Ç' => "C",
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => "E",
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => "I",
        'ñ' | 'Ñ' => "N",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => "O",
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => "U",
        'ý' | 'ÿ' | 'Ý' => "Y",
        'ß' => "SS",
        _ => return None,
    })
}

/// Turn a display name into an `UPPER_SNAKE` id fragment: ASCII letters and digits are
/// kept (upper-cased), apostrophes vanish, `+` becomes `PLUS`, `&` becomes `AND`, any other
/// character is a separator. The result never contains characters that are operators in the
/// expression languages.
#[must_use]
pub fn id_fragment(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_sep = false;
    let push = |out: &mut String, text: &str, pending: &mut bool| {
        if *pending && !out.is_empty() {
            out.push('_');
        }
        *pending = false;
        out.push_str(text);
    };
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            let up = c.to_ascii_uppercase().to_string();
            push(&mut out, &up, &mut pending_sep);
        } else if c == '\'' || c == '\u{2019}' || c == '`' {
            // dropped silently
        } else if c == '+' {
            push(&mut out, "PLUS", &mut pending_sep);
            pending_sep = true;
        } else if c == '&' {
            pending_sep = true;
            push(&mut out, "AND", &mut pending_sep);
            pending_sep = true;
        } else if let Some(folded) = fold_char(c) {
            push(&mut out, folded, &mut pending_sep);
        } else {
            pending_sep = true;
        }
    }
    if out.is_empty() {
        "X".to_string()
    } else {
        out
    }
}

/// Lower-case a key the way 5etools does for lookups (`name|source`).
#[must_use]
pub fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// A `(name, source)` lookup key, case-insensitive.
#[must_use]
pub fn name_source_key(name: &str, source: &str) -> String {
    format!("{}|{}", key(name), key(source))
}

/// HTML-escape text content.
#[must_use]
pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Remove characters that cannot appear in a supports-expression token.
#[must_use]
pub fn supports_token(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ' ' | '.' | '/' | '$' | '-'))
        .collect();
    cleaned.trim().to_string()
}

/// Capitalise the first letter of every word.
#[must_use]
pub fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_start = true;
    for c in text.chars() {
        if at_start && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            at_start = false;
        } else {
            out.push(c);
            if c == ' ' || c == '-' {
                at_start = true;
            } else if c.is_alphanumeric() {
                at_start = false;
            }
        }
    }
    out
}

/// `Some(&str)` for a JSON string.
#[must_use]
pub fn as_str(value: &Value) -> Option<&str> {
    value.as_str()
}

/// True if `value` is a JSON `true` or a non-empty string (how 5etools writes flags such
/// as `"srd": "Acid Splash"`).
#[must_use]
pub fn flag_set(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        _ => false,
    }
}

/// Ordinal suffix text (`1st`, `2nd`, `3rd`, `4th`).
#[must_use]
pub fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragments_are_upper_snake_ascii() {
        assert_eq!(id_fragment("Fire Bolt"), "FIRE_BOLT");
        assert_eq!(id_fragment("Hunter's Mark"), "HUNTERS_MARK");
        assert_eq!(id_fragment("Arrows (20)"), "ARROWS_20");
        assert_eq!(id_fragment("+1 Longsword"), "PLUS_1_LONGSWORD");
        assert_eq!(
            id_fragment("Spell Scroll (Cantrip)"),
            "SPELL_SCROLL_CANTRIP"
        );
        assert_eq!(id_fragment("Tasha’s Mind Whip"), "TASHAS_MIND_WHIP");
        assert_eq!(id_fragment("Éowyn & Co"), "EOWYN_AND_CO");
        assert_eq!(id_fragment("☃"), "X");
        assert_eq!(id_fragment("a--b"), "A_B");
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn helpers() {
        assert_eq!(name_source_key(" Fireball ", "PHB"), "fireball|phb");
        assert_eq!(escape_html("a<b&\"c\">"), "a&lt;b&amp;&quot;c&quot;&gt;");
        assert_eq!(supports_token("Smith’s Tools (x)"), "Smiths Tools x");
        assert_eq!(title_case("animal handling"), "Animal Handling");
        assert_eq!(title_case("pan flute"), "Pan Flute");
        assert!(flag_set(Some(&Value::Bool(true))));
        assert!(flag_set(Some(&Value::String("Acid".into()))));
        assert!(!flag_set(Some(&Value::String(String::new()))));
        assert!(!flag_set(None));
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(3), "3rd");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(9), "9th");
        assert_eq!(as_str(&Value::String("x".into())), Some("x"));
    }
}
