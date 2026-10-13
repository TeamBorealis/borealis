//! Text placeholders: `{{key}}` and `$(key)`.

use std::fmt;

/// Which delimiter style a placeholder used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderSyntax {
    /// `{{key}}`
    Braces,
    /// `$(key)`
    Dollar,
}

/// One placeholder occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    /// Text between the delimiters, verbatim.
    pub key: String,
    /// The delimiter style used.
    pub syntax: PlaceholderSyntax,
}

impl Placeholder {
    /// The key trimmed and lower-cased after the legacy statistic name mapping, i.e. the
    /// group name the engine should look up.
    #[must_use]
    pub fn stat_key(&self) -> String {
        crate::legacy::map_stat_name(self.key.trim())
    }
}

/// A piece of a parsed template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatePart {
    /// Literal text.
    Text(String),
    /// A placeholder to substitute.
    Placeholder(Placeholder),
}

/// Text split into literal runs and placeholders. `Display` reproduces the input exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    parts: Vec<TemplatePart>,
}

impl Template {
    /// Split `text` into literal runs and placeholders. Never fails: an unterminated
    /// `{{` or `$(` is plain text.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut parts = Vec::new();
        let mut literal = String::new();
        let mut rest = text;
        while !rest.is_empty() {
            if let Some((ph, remaining)) = Self::take_placeholder(rest) {
                if !literal.is_empty() {
                    parts.push(TemplatePart::Text(std::mem::take(&mut literal)));
                }
                parts.push(TemplatePart::Placeholder(ph));
                rest = remaining;
            } else {
                let mut chars = rest.chars();
                if let Some(c) = chars.next() {
                    literal.push(c);
                }
                rest = chars.as_str();
            }
        }
        if !literal.is_empty() {
            parts.push(TemplatePart::Text(literal));
        }
        Self { parts }
    }

    fn take_placeholder(rest: &str) -> Option<(Placeholder, &str)> {
        let (open, close, syntax) = if rest.starts_with("{{") {
            ("{{", "}}", PlaceholderSyntax::Braces)
        } else if rest.starts_with("$(") {
            ("$(", ")", PlaceholderSyntax::Dollar)
        } else {
            return None;
        };
        let body = &rest[open.len()..];
        let end = body.find(close)?;
        let key = &body[..end];
        if key.is_empty() {
            return None;
        }
        Some((
            Placeholder {
                key: key.to_string(),
                syntax,
            },
            &body[end + close.len()..],
        ))
    }

    /// The parts in order.
    #[must_use]
    pub fn parts(&self) -> &[TemplatePart] {
        &self.parts
    }

    /// Only the placeholders, in order.
    #[must_use]
    pub fn placeholders(&self) -> Vec<&Placeholder> {
        self.parts
            .iter()
            .filter_map(|p| match p {
                TemplatePart::Placeholder(ph) => Some(ph),
                TemplatePart::Text(_) => None,
            })
            .collect()
    }

    /// True if no placeholder is present.
    #[must_use]
    pub fn is_literal(&self) -> bool {
        self.placeholders().is_empty()
    }
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for part in &self.parts {
            match part {
                TemplatePart::Text(t) => f.write_str(t)?,
                TemplatePart::Placeholder(ph) => match ph.syntax {
                    PlaceholderSyntax::Braces => write!(f, "{{{{{}}}}}", ph.key)?,
                    PlaceholderSyntax::Dollar => write!(f, "$({})", ph.key)?,
                },
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_both_syntaxes_and_round_trips() {
        let src = "You can prepare {{wizard:spellcasting:prepare}} spells, DC $(ki:dc)!";
        let t = Template::parse(src);
        let keys: Vec<_> = t.placeholders().iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, ["wizard:spellcasting:prepare", "ki:dc"]);
        assert_eq!(t.to_string(), src);
        assert!(!t.is_literal());
    }

    #[test]
    fn unterminated_or_empty_is_literal() {
        for src in ["a {{b", "cost $(5", "{{}}", "no placeholders", ""] {
            let t = Template::parse(src);
            assert!(t.is_literal(), "{src}");
            assert_eq!(t.to_string(), src);
        }
    }

    #[test]
    fn stat_key_applies_legacy_mapping() {
        let t = Template::parse("{{Strength Modifier}} and {{Level:Fighter}}");
        let keys: Vec<_> = t.placeholders().iter().map(|p| p.stat_key()).collect();
        assert_eq!(keys, ["strength:modifier", "level:fighter"]);
    }

    #[test]
    fn handles_multibyte_text() {
        let src = "Player’s {{x}} – done";
        assert_eq!(Template::parse(src).to_string(), src);
    }
}
