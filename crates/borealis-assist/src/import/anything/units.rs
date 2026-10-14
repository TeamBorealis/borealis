//! Costs and weights in the bundled packs' form: a `cost` setter is the number with a
//! `currency` attribute, a `weight` setter reads "2 lb." with an `lb` attribute, and text
//! writes "25 gp" and "3 lb." (a space, "lb." for pounds).

use serde_json::{json, Value};

const COINS: [&str; 5] = ["gp", "sp", "cp", "ep", "pp"];

/// `(amount, coin)` of "25gp", "1,000 gp", "25" (amount without separators, as the bundled
/// setters write it); the coin defaults to `fallback`.
fn split_cost(s: &str, fallback: &str) -> Option<(String, String)> {
    let t = s.trim();
    let amount: String = t
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '.')
        .collect();
    if amount.is_empty() {
        return None;
    }
    let coin = t[amount.len()..].trim().to_lowercase();
    let coin = coin.trim_end_matches('.');
    let amount = amount.replace(',', "");
    if coin.is_empty() {
        return Some((amount, fallback.to_string()));
    }
    COINS.contains(&coin).then(|| (amount, coin.to_string()))
}

/// Pounds of "3", "3 lb", "3 lbs.", "3 pounds", "1/2 lb.".
fn pounds(s: &str) -> Option<String> {
    let t = s.trim().to_lowercase();
    let amount: String = t
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '/' | ','))
        .collect();
    let unit = t[amount.len()..].trim().trim_end_matches('.');
    (!amount.is_empty() && matches!(unit, "" | "lb" | "lbs" | "pound" | "pounds")).then_some(amount)
}

fn setter_mut<'a>(el: &'a mut Value, name: &str) -> Option<&'a mut Value> {
    el.get_mut("setters")?
        .as_array_mut()?
        .iter_mut()
        .find(|s| s.get("name").and_then(Value::as_str) == Some(name))
}

fn tidy_setters(el: &mut Value) {
    if let Some(s) = setter_mut(el, "cost") {
        let fallback = s
            .pointer("/attributes/currency")
            .and_then(Value::as_str)
            .unwrap_or("gp")
            .to_string();
        let value = s.get("value").map(value_text).unwrap_or_default();
        if let Some((amount, coin)) = split_cost(&value, &fallback) {
            s["value"] = json!(amount);
            s["attributes"] = json!({"currency": coin});
        }
    }
    if let Some(s) = setter_mut(el, "weight") {
        let value = s.get("value").map(value_text).unwrap_or_default();
        if let Some(lb) = pounds(&value) {
            s["value"] = json!(format!("{lb} lb."));
            s["attributes"] = json!({"lb": lb});
        }
    }
}

fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// "25gp" -> "25 gp", "3lbs." / "3 lbs" -> "3 lb." in running text.
pub(crate) fn tidy_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        out.push(chars[i]);
        let digit = chars[i].is_ascii_digit();
        i += 1;
        if !digit || chars.get(i).is_some_and(char::is_ascii_digit) {
            continue;
        }
        let mut j = i;
        while chars.get(j) == Some(&' ') {
            j += 1;
        }
        let word: String = chars[j..]
            .iter()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        let end = j + word.len();
        let boundary = !chars.get(end).is_some_and(|c| c.is_alphanumeric());
        if !boundary {
            continue;
        }
        if COINS.contains(&word.as_str()) && j == i {
            out.push(' ');
        } else if word == "lb" || word == "lbs" {
            out.push_str(" lb.");
            i = if chars.get(end) == Some(&'.') {
                end + 1
            } else {
                end
            };
        }
    }
    out
}

/// Bring the element's cost and weight setters and the units in its description to the
/// bundled form.
pub(crate) fn apply(el: &mut Value) {
    tidy_setters(el);
    if let Some(d) = el.get("description").and_then(Value::as_str) {
        let tidy = tidy_text(d);
        if tidy != d {
            el["description"] = json!(tidy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn costs_and_weights_take_the_bundled_form() {
        assert_eq!(
            tidy_text("a gem worth 25gp, 1,000 gp of dust, weighs 3lbs. or 2 lb or 4 lbs"),
            "a gem worth 25 gp, 1,000 gp of dust, weighs 3 lb. or 2 lb. or 4 lb."
        );
        assert_eq!(tidy_text("level 3rd, 2d6 gpx"), "level 3rd, 2d6 gpx");
        let mut el = json!({"description": "<p>Costs 5gp.</p>", "setters": [
            {"name": "cost", "value": "1,500gp"}, {"name": "weight", "value": 2}]});
        apply(&mut el);
        assert_eq!(el["description"], "<p>Costs 5 gp.</p>");
        assert_eq!(
            el["setters"][0],
            json!({"name": "cost", "value": "1500",
            "attributes": {"currency": "gp"}})
        );
        assert_eq!(
            el["setters"][1],
            json!({"name": "weight", "value": "2 lb.",
            "attributes": {"lb": "2"}})
        );
        let mut odd = json!({"setters": [{"name": "cost", "value": "varies"}]});
        apply(&mut odd);
        assert_eq!(odd["setters"][0]["value"], "varies");
    }
}
