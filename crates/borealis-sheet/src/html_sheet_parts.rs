//! Small HTML building blocks shared by the page builders of the HTML sheet.

use super::HtmlOptions;
use crate::model_types::{AbilityData, SkillData};
use std::fmt::Write;

/// Escape text for HTML content and attribute values.
pub(super) fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// A value element carrying its field id. `cls` is extra CSS classes.
pub(super) fn fld(id: &str, cls: &str, value: &str) -> String {
    format!(
        "<span class=\"fv {cls}\" data-field=\"{}\" dir=\"auto\">{}</span>",
        esc(id),
        esc(value)
    )
}

/// A write-in box (play state, left empty).
pub(super) fn blank(id: &str, cls: &str) -> String {
    format!(
        "<span class=\"f blank {cls}\" data-field=\"{}\"></span>",
        esc(id)
    )
}

/// A round pip; `state` is `on`, `ex`, `na` or empty.
pub(super) fn pip(id: &str, state: &str) -> String {
    format!(
        "<span class=\"pip {state}\" data-field=\"{}\"></span>",
        esc(id)
    )
}

/// An empty or ticked square box.
pub(super) fn sq(id: &str, on: bool) -> String {
    format!(
        "<span class=\"sq{}\" data-field=\"{}\"></span>",
        if on { " on" } else { "" },
        esc(id)
    )
}

/// A track dot.
pub(super) fn dot(id: &str, cls: &str) -> String {
    format!(
        "<span class=\"dot {cls}\" data-field=\"{}\"></span>",
        esc(id)
    )
}

/// A multi-line text area on ruled lines, pre-filled with `text`.
pub(super) fn area(id: &str, text: &str, small: bool) -> String {
    format!(
        "<div class=\"lined{}\" data-field=\"{}\" dir=\"auto\">{}</div>",
        if small { " sm" } else { "" },
        esc(id),
        esc(text.trim())
    )
}

/// Section heading with the ornamental rule and an optional aside (trusted HTML).
pub(super) fn h2(title: &str, aside: &str) -> String {
    let aside = if aside.is_empty() {
        String::new()
    } else {
        format!("<span class=\"aside\">{aside}</span>")
    };
    format!(
        "<h2>{}{aside}<svg class=\"hr\" aria-hidden=\"true\"><use href=\"#a-header-rule\" width=\"100%\" height=\"100%\"/></svg></h2>",
        esc(title)
    )
}

/// `<svg>` that instantiates an art symbol.
pub(super) fn art(symbol: &str, cls: &str) -> String {
    format!(
        "<svg class=\"{cls}\" aria-hidden=\"true\"><use href=\"#a-{symbol}\" width=\"100%\" height=\"100%\"/></svg>"
    )
}

/// `1234567` as `1,234,567`.
pub(super) fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Pounds without a needless `.0`.
pub(super) fn pounds(w: f64) -> String {
    let t = format!("{w:.1}");
    t.strip_suffix(".0").unwrap_or(&t).to_string()
}

/// `2nd level`, `3rd level` ...
pub(super) fn level_name(level: u32) -> String {
    let suffix = match (level % 10, level % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{level}{suffix} level")
}

/// Ability card: sigil, name, modifier, score circle, save row and `skills` nested under it.
pub(super) fn ability_card(
    o: &HtmlOptions,
    a: &AbilityData,
    skills: &[&SkillData],
    signed: &dyn Fn(i32) -> String,
) -> String {
    use crate::model_ids as id;
    let mut out = format!(
        "<div class=\"ab\"><div class=\"ab-top\">{}<div class=\"ab-name\">{}<small>{}</small></div>",
        art(&format!("sigil-{}", a.key), ""),
        esc(&a.name),
        a.key
    );
    let modifier = fld(&id::abil_mod(a.key), "", &signed(a.modifier));
    let score = fld(&id::abil_score(a.key), "", &a.score.to_string());
    let _ = if o.swap_ability_positions {
        write!(
            out,
            "<div class=\"score\">{modifier}</div><div class=\"mod\">{score}</div>"
        )
    } else {
        write!(
            out,
            "<div class=\"mod\">{modifier}</div><div class=\"score\">{score}</div>"
        )
    };
    let _ = write!(
        out,
        "</div><div class=\"rows\"><div class=\"r save\">{}<span>Saving throw</span>{}</div>",
        pip(
            &id::save_prof(a.key),
            if a.save_proficient { "on" } else { "" }
        ),
        fld(&id::save(a.key), "v", &signed(a.save)),
    );
    for s in skills {
        let _ = write!(
            out,
            "<div class=\"r\">{}<span>{}</span>{}</div>",
            skill_mark(s),
            esc(&s.name),
            fld(&id::skill(&s.key), "v", &signed(s.total))
        );
    }
    out.push_str("</div></div>");
    out
}

/// Proficiency / expertise marker of a skill (both field ids, one visible).
pub(super) fn skill_mark(s: &SkillData) -> String {
    use crate::model_ids as id;
    format!(
        "<span class=\"mk{}\">{}{}</span>",
        if s.expertise { " ex" } else { "" },
        pip(
            &id::skill_prof(&s.key),
            if s.proficient { "on p" } else { "p" }
        ),
        pip(&id::skill_exp(&s.key), "ex e"),
    )
}
