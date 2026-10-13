//! `{@tag arg|arg|display}` markup of 5etools entries.

use crate::util::escape_html;

/// Kinds of entities a tag can link to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefKind {
    /// `{@spell}`
    Spell,
    /// `{@item}`
    Item,
    /// `{@feat}`
    Feat,
    /// `{@race}`
    Race,
    /// `{@class}`
    Class,
    /// `{@background}`
    Background,
    /// `{@optfeature}`
    OptionalFeature,
}

impl RefKind {
    /// Source assumed by 5etools when the tag omits it.
    #[must_use]
    pub fn default_source(self) -> &'static str {
        match self {
            Self::Item => "DMG",
            _ => "PHB",
        }
    }
}

/// Split a tag body on `|` at brace depth 0.
#[must_use]
pub fn split_args(body: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            '|' if depth == 0 => {
                parts.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[start..]);
    parts
}

/// What a tag renders to, before HTML escaping of plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rendered {
    /// Plain text (to be escaped).
    Text(String),
    /// Text that links to an entity.
    Link {
        /// Which kind of entity.
        kind: RefKind,
        /// Entity name.
        name: String,
        /// Entity source (explicit or the default).
        source: String,
        /// Display text.
        text: String,
    },
    /// Inline markup around nested content: `(open, inner_raw, close)`.
    Wrap(&'static str, String, &'static str),
}

fn non_empty<'a>(parts: &[&'a str], idx: usize) -> Option<&'a str> {
    parts
        .get(idx)
        .copied()
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn display(parts: &[&str], idx: usize) -> String {
    non_empty(parts, idx)
        .or_else(|| non_empty(parts, 0))
        .unwrap_or("")
        .to_string()
}

fn link(kind: RefKind, parts: &[&str], display_idx: usize) -> Rendered {
    let name = non_empty(parts, 0).unwrap_or("").to_string();
    let source = non_empty(parts, 1)
        .unwrap_or_else(|| kind.default_source())
        .to_uppercase();
    let text = display(parts, display_idx);
    Rendered::Link {
        kind,
        name,
        source,
        text,
    }
}

fn atk_text(code: &str) -> String {
    let mut parts = Vec::new();
    for c in code.split(',') {
        match c.trim() {
            "m" => parts.push("Melee"),
            "r" => parts.push("Ranged"),
            "mw" => parts.push("Melee Weapon"),
            "rw" => parts.push("Ranged Weapon"),
            "ms" => parts.push("Melee Spell"),
            "rs" => parts.push("Ranged Spell"),
            "ma" => parts.push("Melee Attack"),
            "ra" => parts.push("Ranged Attack"),
            _ => {}
        }
    }
    let joined = parts.join(" or ");
    if joined.contains("Attack") {
        format!("{joined}:")
    } else {
        format!("{joined} Attack:")
    }
}

fn signed(n: &str) -> String {
    let n = n.trim();
    if n.starts_with('+') || n.starts_with('-') || n.starts_with('\u{2212}') {
        n.to_string()
    } else {
        format!("+{n}")
    }
}

/// Render one tag. `body` is everything after `@tag ` (may be empty).
#[must_use]
pub fn render_tag(tag: &str, body: &str) -> Rendered {
    let parts = split_args(body);
    let text = |s: String| Rendered::Text(s);
    match tag {
        "b" | "bold" => Rendered::Wrap("<strong>", body.to_string(), "</strong>"),
        "i" | "italic" => Rendered::Wrap("<em>", body.to_string(), "</em>"),
        "s" | "strike" => Rendered::Wrap("<s>", body.to_string(), "</s>"),
        "u" | "underline" => Rendered::Wrap("<u>", body.to_string(), "</u>"),
        "sup" => Rendered::Wrap("<sup>", body.to_string(), "</sup>"),
        "sub" => Rendered::Wrap("<sub>", body.to_string(), "</sub>"),
        "code" | "kbd" => Rendered::Wrap("<code>", body.to_string(), "</code>"),
        "note" => Rendered::Wrap("<em class=\"note\">", body.to_string(), "</em>"),
        "tip" | "footnote" | "color" | "font" | "highlight" | "comic" | "comicH1" | "comicH2"
        | "comicH3" | "comicH4" | "comicNote" => {
            Rendered::Wrap("", parts.first().copied().unwrap_or("").to_string(), "")
        }
        "spell" => link(RefKind::Spell, &parts, 2),
        "item" => link(RefKind::Item, &parts, 2),
        "feat" => link(RefKind::Feat, &parts, 2),
        "race" => link(RefKind::Race, &parts, 2),
        "class" => link(RefKind::Class, &parts, 2),
        "background" => link(RefKind::Background, &parts, 2),
        "optfeature" => link(RefKind::OptionalFeature, &parts, 2),
        "creature" | "condition" | "disease" | "status" | "reward" | "vehicle" | "object"
        | "trap" | "hazard" | "language" | "sense" | "skill" | "action" | "variantrule"
        | "table" | "cult" | "boon" | "psionic" | "itemMastery" | "itemProperty"
        | "optfeature2" => text(display(&parts, 2)),
        "deity" => text(display(&parts, 3)),
        "card" => text(display(&parts, 3)),
        "classFeature" => text(display(&parts, 5)),
        "subclassFeature" => text(display(&parts, 7)),
        "quickref" => text(display(&parts, 4)),
        "subclass" => text(display(&parts, 2)),
        "book" | "adventure" | "filter" | "link" | "5etools" | "homebrew" => {
            text(display(&parts, 0))
        }
        "dice" | "damage" | "autodice" => text(display(&parts, 1)),
        "scaledice" | "scaledamage" => text(display(&parts, 2)),
        "d20" | "hit" => text(signed(&display(&parts, 1))),
        "dc" => text(non_empty(&parts, 1).map_or_else(
            || format!("DC {}", non_empty(&parts, 0).unwrap_or("")),
            str::to_string,
        )),
        "chance" => text(format!("{} percent", display(&parts, 0))),
        "recharge" => {
            let n = non_empty(&parts, 0).unwrap_or("6");
            if n == "6" {
                text("(Recharge 6)".to_string())
            } else {
                text(format!("(Recharge {n}\u{2013}6)"))
            }
        }
        "atk" => text(atk_text(body)),
        "atkr" => text(atk_text(body)),
        "h" => text("Hit: ".to_string()),
        "m" => text("Miss: ".to_string()),
        "hom" => text("Hit or Miss: ".to_string()),
        "coinflip" => text("flip a coin".to_string()),
        "unit" => text(format!(
            "{} {}",
            non_empty(&parts, 0).unwrap_or(""),
            non_empty(&parts, 1).unwrap_or("")
        )),
        "area" => text(display(&parts, 1)),
        _ => text(display(&parts, 0)),
    }
}

/// Plain text of a rendered tag (used by [`strip_tags`]).
#[must_use]
pub fn rendered_plain(r: &Rendered) -> String {
    match r {
        Rendered::Text(t) => t.clone(),
        Rendered::Link { text, .. } => text.clone(),
        Rendered::Wrap(_, inner, _) => strip_tags(inner),
    }
}

fn find_tag_end(s: &str, from: usize) -> Option<usize> {
    let mut depth = 1usize;
    for (i, c) in s[from..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(from + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Replace all `{@tag ...}` markup with its plain text (no HTML).
#[must_use]
pub fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find("{@") {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 2..];
        let Some(end) = find_tag_end(after, 0) else {
            out.push_str(&rest[pos..]);
            return out;
        };
        let inner = &after[..end];
        let (tag, body) = inner.split_once(' ').unwrap_or((inner, ""));
        out.push_str(&strip_tags(&rendered_plain(&render_tag(tag, body))));
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Escape text for HTML.
#[must_use]
pub fn esc(text: &str) -> String {
    escape_html(text)
}

/// Render inline text containing tags into an HTML fragment. `link_for` maps a link target
/// to an element id.
pub fn render_inline(s: &str, link_for: &dyn Fn(RefKind, &str, &str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    let mut rest = s;
    while let Some(pos) = rest.find("{@") {
        out.push_str(&esc(&rest[..pos]));
        let after = &rest[pos + 2..];
        let Some(end) = find_tag_end(after, 0) else {
            out.push_str(&esc(&rest[pos..]));
            return out;
        };
        let inner = &after[..end];
        let (tag, body) = inner.split_once(' ').unwrap_or((inner, ""));
        match render_tag(tag, body) {
            Rendered::Text(t) => out.push_str(&render_inline(&t, link_for)),
            Rendered::Wrap(open, inner_raw, close) => {
                out.push_str(open);
                out.push_str(&render_inline(&inner_raw, link_for));
                out.push_str(close);
            }
            Rendered::Link {
                kind,
                name,
                source,
                text,
            } => match link_for(kind, &name, &source) {
                Some(id) => {
                    out.push_str("<a class=\"ref\" data-element=\"");
                    out.push_str(&esc(&id));
                    out.push_str("\">");
                    out.push_str(&render_inline(&text, link_for));
                    out.push_str("</a>");
                }
                None => out.push_str(&render_inline(&text, link_for)),
            },
        }
        rest = &after[end + 1..];
    }
    out.push_str(&esc(rest));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_links(_: RefKind, _: &str, _: &str) -> Option<String> {
        None
    }

    #[test]
    fn splits_respecting_braces() {
        assert_eq!(split_args("a|b|c"), vec!["a", "b", "c"]);
        assert_eq!(split_args("a|{@b x|y}|c"), vec!["a", "{@b x|y}", "c"]);
        assert_eq!(split_args(""), vec![""]);
    }

    #[test]
    fn text_tags() {
        assert_eq!(strip_tags("Deal {@damage 8d6} fire."), "Deal 8d6 fire.");
        assert_eq!(strip_tags("{@dice 1d6 + 2|1d6+2|label}"), "1d6+2");
        assert_eq!(strip_tags("{@dice 2d4 + 2}"), "2d4 + 2");
        assert_eq!(strip_tags("{@dc 15}"), "DC 15");
        assert_eq!(strip_tags("{@dc 15|DC 16}"), "DC 16");
        assert_eq!(strip_tags("{@hit 5}"), "+5");
        assert_eq!(strip_tags("{@hit -1}"), "-1");
        assert_eq!(strip_tags("{@atk mw}{@h}3"), "Melee Weapon Attack:Hit: 3");
        assert_eq!(
            strip_tags("{@atk ms,rs}"),
            "Melee Spell or Ranged Spell Attack:"
        );
        assert_eq!(strip_tags("{@spell Fire Bolt|XPHB}"), "Fire Bolt");
        assert_eq!(
            strip_tags("{@spell fire bolt|phb|bolt of fire}"),
            "bolt of fire"
        );
        assert_eq!(strip_tags("{@condition Poisoned|XPHB}"), "Poisoned");
        assert_eq!(
            strip_tags("{@variantrule Hit Points|XPHB|Hit Point}"),
            "Hit Point"
        );
        assert_eq!(strip_tags("{@scaledamage 8d6|3-9|1d6}"), "1d6");
        assert_eq!(strip_tags("{@filter cantrips|spells|level=0}"), "cantrips");
        assert_eq!(strip_tags("{@book chapter 7|XPHB|7}"), "chapter 7");
        assert_eq!(strip_tags("{@recharge 5}"), "(Recharge 5\u{2013}6)");
        assert_eq!(strip_tags("{@recharge}"), "(Recharge 6)");
        assert_eq!(strip_tags("{@chance 25}"), "25 percent");
        assert_eq!(strip_tags("{@unit 25|ft.}"), "25 ft.");
        assert_eq!(strip_tags("{@quickref x||||shown}"), "shown");
        assert_eq!(
            strip_tags("{@classFeature Rage|Barbarian||1||Rage!}"),
            "Rage!"
        );
        assert_eq!(strip_tags("{@unknownTag foo|bar}"), "foo");
        assert_eq!(strip_tags("{@note see {@spell Light}}"), "see Light");
        assert_eq!(
            strip_tags("unterminated {@spell x"),
            "unterminated {@spell x"
        );
        assert_eq!(strip_tags("plain"), "plain");
        assert_eq!(strip_tags("{@d20 3}"), "+3");
        assert_eq!(strip_tags("{@coinflip}"), "flip a coin");
        assert_eq!(strip_tags("{@deity Pelor|Greyhawk|PSA|the Sun}"), "the Sun");
    }

    #[test]
    fn html_rendering() {
        assert_eq!(
            render_inline("A {@b bold} & {@i it}", &no_links),
            "A <strong>bold</strong> &amp; <em>it</em>"
        );
        assert_eq!(render_inline("x < y", &no_links), "x &lt; y");
        assert_eq!(
            render_inline("{@note n {@b b}}", &no_links),
            "<em class=\"note\">n <strong>b</strong></em>"
        );
    }

    #[test]
    fn links_use_the_resolver() {
        let resolver = |kind: RefKind, name: &str, source: &str| {
            if kind == RefKind::Spell && name.eq_ignore_ascii_case("fireball") {
                Some(format!("ID_{source}_FIREBALL"))
            } else {
                None
            }
        };
        assert_eq!(
            render_inline("cast {@spell Fireball|XPHB} or {@spell Light}", &resolver),
            "cast <a class=\"ref\" data-element=\"ID_XPHB_FIREBALL\">Fireball</a> or Light"
        );
        assert_eq!(
            render_inline("{@spell fireball}", &resolver),
            "<a class=\"ref\" data-element=\"ID_PHB_FIREBALL\">fireball</a>"
        );
        assert_eq!(RefKind::Item.default_source(), "DMG");
    }
}
