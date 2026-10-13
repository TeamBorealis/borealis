//! Cantrip damage scaling by character level (2024 `Cantrip Upgrade`). The SRD spell text is
//! the only source: the base dice come from the first `NdM <Type> damage` phrase and the tiers
//! from the pairs `level (dice)` that follow `Cantrip Upgrade`.

use crate::play_rules::plain_text;

/// A parsed `NdM` token: `(start, end)` byte range in the text.
fn dice_at(text: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'd' {
                let mut k = j + 1;
                while k < bytes.len() && bytes[k].is_ascii_digit() {
                    k += 1;
                }
                if k > j + 1 {
                    return Some((i, k));
                }
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    None
}

/// Base dice and damage type of the first `NdM Type damage` phrase.
fn base_damage(text: &str) -> Option<(String, String)> {
    let mut from = 0;
    while let Some((s, e)) = dice_at(text, from) {
        let mut words = text[e..].split_whitespace();
        let (ty, damage) = (words.next(), words.next());
        if damage.is_some_and(|d| d.starts_with("damage")) {
            return Some((text[s..e].to_string(), ty.unwrap_or("").to_string()));
        }
        from = e;
    }
    None
}

/// Tiers `(level, dice)` from the `Cantrip Upgrade` sentence: `levels 5 (2d10), 11 (3d10)`.
fn tiers(text: &str) -> Vec<(u32, String)> {
    let Some(at) = text.find("Cantrip Upgrade") else {
        return Vec::new();
    };
    let tail = &text[at..];
    let bytes = tail.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            let level = tail[i..j].parse::<u32>().ok();
            let rest = tail[j..].trim_start();
            if let (Some(l), Some(inner)) = (level, rest.strip_prefix('(')) {
                if let Some((s, e)) = dice_at(inner, 0) {
                    if s == 0 {
                        out.push((l, inner[..e].to_string()));
                    }
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Damage of a cantrip at a character level, e.g. `2d10 Fire`; `None` when the text has no
/// plain dice scaling (Eldritch Blast adds beams, utility cantrips deal no damage).
#[must_use]
pub fn scaled_damage(description_html: &str, character_level: u32) -> Option<String> {
    let text = plain_text(description_html);
    if !text.contains("Cantrip Upgrade") {
        return None;
    }
    let (base, ty) = base_damage(&text)?;
    let tiers = tiers(&text);
    if tiers.is_empty() {
        return None;
    }
    let dice = tiers
        .iter()
        .filter(|(l, _)| *l <= character_level)
        .max_by_key(|(l, _)| *l)
        .map_or(base, |(_, d)| d.clone());
    Some(format!("{dice} {ty}").trim().to_string())
}

/// Dice from a `levels` map keyed by character level (`{"1": "1d10", "5": "2d10"}`).
#[must_use]
pub fn dice_for_level(
    levels: &serde_json::Map<String, serde_json::Value>,
    level: u32,
) -> Option<String> {
    levels
        .iter()
        .filter_map(|(k, v)| Some((k.parse::<u32>().ok()?, v.as_str()?)))
        .filter(|(l, _)| *l <= level)
        .max_by_key(|(l, _)| *l)
        .map(|(_, d)| d.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRE_BOLT: &str = "<p>On a hit, the target takes 1d10 Fire damage. A flammable object.</p>\n<p><b>Cantrip Upgrade.</b> The damage increases by 1d10 when you reach levels 5 (2d10), 11 (3d10), and 17 (4d10).</p>";

    #[test]
    fn fire_bolt_tiers() {
        assert_eq!(scaled_damage(FIRE_BOLT, 1).as_deref(), Some("1d10 Fire"));
        assert_eq!(scaled_damage(FIRE_BOLT, 4).as_deref(), Some("1d10 Fire"));
        assert_eq!(scaled_damage(FIRE_BOLT, 5).as_deref(), Some("2d10 Fire"));
        assert_eq!(scaled_damage(FIRE_BOLT, 11).as_deref(), Some("3d10 Fire"));
        assert_eq!(scaled_damage(FIRE_BOLT, 20).as_deref(), Some("4d10 Fire"));
    }

    #[test]
    fn beams_have_no_dice_tier() {
        let t = "<p>deals 1d10 Force damage.</p><p><b>Cantrip Upgrade.</b> The spell creates two beams at level 5.</p>";
        assert_eq!(scaled_damage(t, 5), None);
    }
}
