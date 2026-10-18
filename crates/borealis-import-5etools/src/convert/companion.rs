//! A creature the character chooses, stated as a 5etools bestiary filter (PHB Ranger's
//! Companion: "Choose a {@filter <text>|bestiary|challenge rating=[&0;&1/4]|type=beast|
//! size=f;d;t;s;m|miscellaneous=!swarm}"), becomes a `Companion` select whose supports are the
//! filter's own fields: `Beast,(Tiny||Small||Medium),(0||1/8||1/4)`. The engine offers every loaded
//! `Companion` or typed `Monster` element those tags fit (creature type, size, challenge
//! rating); the 5etools import itself brings no creatures (the bestiary is not imported), so
//! the app offers the bundled SRD stat blocks.
//!
//! Narrow and deterministic (never AI, IM1): only a top-level sentence "Choose a(n) {@filter
//! ...|bestiary|...}" is read, and only when every filter field is one of `type`, `size`,
//! `challenge rating` and `miscellaneous=!swarm`. Swarms cannot be told apart by the tags the
//! engine gives a stat block, so `!swarm` is not enforced.

use super::build::select;
use crate::util::title_case;
use borealis_content::Rule;
use regex::Regex;
use serde_json::Value;

/// Challenge ratings in ascending order.
const CR_LADDER: [&str; 34] = [
    "0", "1/8", "1/4", "1/2", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13",
    "14", "15", "16", "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29",
    "30",
];

fn cr_value(cr: &str) -> Option<f64> {
    match cr.split_once('/') {
        Some((n, d)) => Some(n.trim().parse::<f64>().ok()? / d.trim().parse::<f64>().ok()?),
        None => cr.trim().parse().ok(),
    }
}

/// `[&0;&1/4]` (a range) or `1/4;1/2` (a list): the challenge ratings it holds.
fn challenge_ratings(v: &str) -> Option<Vec<&'static str>> {
    if let Some(range) = v.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        let (lo, hi) = range.split_once(';')?;
        let (lo, hi) = (
            cr_value(lo.trim_start_matches('&'))?,
            cr_value(hi.trim_start_matches('&'))?,
        );
        let out: Vec<&str> = CR_LADDER
            .into_iter()
            .filter(|c| cr_value(c).is_some_and(|x| x >= lo && x <= hi))
            .collect();
        return (!out.is_empty()).then_some(out);
    }
    v.split(';')
        .map(|c| CR_LADDER.into_iter().find(|l| *l == c.trim()))
        .collect()
}

/// 5etools size letters; Fine and Diminutive have no 5e creature and are left out.
fn sizes(v: &str) -> Option<Vec<&'static str>> {
    let mut out = Vec::new();
    for letter in v.split(';') {
        let size = match letter.trim().to_ascii_lowercase().as_str() {
            "f" | "d" => continue,
            "t" => "Tiny",
            "s" => "Small",
            "m" => "Medium",
            "l" => "Large",
            "h" => "Huge",
            "g" => "Gargantuan",
            _ => return None,
        };
        out.push(size);
    }
    (!out.is_empty()).then_some(out)
}

fn group(terms: &[String]) -> String {
    if terms.len() == 1 {
        terms[0].clone()
    } else {
        format!("({})", terms.join("||"))
    }
}

/// The supports expression of a bestiary filter's fields (`challenge rating=[&0;&1/4]|
/// type=beast|...`), or `None` when a field cannot be stated as tags.
#[must_use]
pub fn filter_supports(fields: &str) -> Option<String> {
    let (mut types, mut size, mut cr) = (None, None, None);
    for field in fields.split('|') {
        let (k, v) = field.split_once('=')?;
        match k.trim().to_ascii_lowercase().as_str() {
            "type" if !v.contains('!') => {
                types = Some(v.split(';').map(title_case).collect::<Vec<_>>());
            }
            "size" => size = Some(sizes(v)?),
            "challenge rating" => cr = Some(challenge_ratings(v)?),
            "miscellaneous" if v.trim().eq_ignore_ascii_case("!swarm") => {}
            _ => return None,
        }
    }
    let mut groups = vec![group(&types?)];
    for terms in [size, cr].into_iter().flatten() {
        groups.push(group(
            &terms.into_iter().map(str::to_string).collect::<Vec<_>>(),
        ));
    }
    Some(groups.join(","))
}

/// The `Companion` select of a feature whose own text says "Choose a {@filter ...|bestiary|...}".
#[must_use]
pub fn companion_select(feature: &str, entries: &[Value]) -> Option<Rule> {
    let re = Regex::new(r"(?i)\bchoose an? \{@filter [^|}]*\|bestiary\|([^}]*)\}").ok()?;
    let supports = entries
        .iter()
        .filter_map(Value::as_str)
        .find_map(|s| re.captures(s).and_then(|c| filter_supports(&c[1])))?;
    Some(Rule::Select(select(
        "Companion",
        feature,
        Some(&supports),
        1,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_rangers_companion_filter_becomes_companion_supports() {
        let text = "A companion joins you. Choose a {@filter small beast|bestiary|challenge \
            rating=[&0;&1/4]|type=beast|size=f;d;t;s;m|miscellaneous=!swarm}. It obeys you.";
        let Some(Rule::Select(s)) = companion_select("Companion Feature", &[json!(text)]) else {
            panic!("companion select");
        };
        assert_eq!(s.ty, "Companion");
        assert_eq!(s.name, "Companion Feature");
        assert_eq!(
            s.supports.as_deref(),
            Some("Beast,(Tiny||Small||Medium),(0||1/8||1/4)")
        );
    }

    #[test]
    fn other_bestiary_mentions_are_not_a_choice() {
        // Wild Shape tables, Destroy Undead and Favored Enemy name bestiary filters too
        for text in [
            "When an {@filter undead|bestiary|challenge rating=[&0;&1/2]|type=undead} fails \
             its save, it is destroyed.",
            "Choose a type of foe: {@filter aberrations|bestiary|type=aberration}, ...",
            "You become a {@filter beast|bestiary|challenge rating=[&0;&1]|type=beast}.",
        ] {
            assert!(companion_select("X", &[json!(text)]).is_none(), "{text}");
        }
        // a field that cannot be stated as tags keeps the whole select out
        assert_eq!(
            filter_supports("challenge rating=[&0;&1/4]|type=beast|speed type=!fly"),
            None
        );
        assert_eq!(filter_supports("type=!beast"), None);
        assert_eq!(
            filter_supports("type=beast;fey|challenge rating=1/2;1"),
            Some("(Beast||Fey),(1/2||1)".into())
        );
    }
}
