//! Small constructors for rules and elements.

use borealis_content::{GrantRule, Rule, SelectRule, Setter, StatRule};

/// `grant` rule.
#[must_use]
pub fn grant(id: &str, ty: &str) -> Rule {
    Rule::Grant(GrantRule {
        id: id.to_string(),
        ty: Some(ty.to_string()),
        ..GrantRule::default()
    })
}

/// `grant` rule active from `level`.
#[must_use]
pub fn grant_at(id: &str, ty: &str, level: u32) -> Rule {
    Rule::Grant(GrantRule {
        id: id.to_string(),
        ty: Some(ty.to_string()),
        level: (level > 1).then_some(level),
        ..GrantRule::default()
    })
}

/// `stat` rule.
#[must_use]
pub fn stat(name: &str, value: impl ToString) -> StatRule {
    StatRule {
        name: name.to_string(),
        value: value.to_string(),
        ..StatRule::default()
    }
}

/// `stat` rule with a stacking class.
#[must_use]
pub fn stat_bonus(name: &str, value: impl ToString, bonus: &str) -> Rule {
    Rule::Stat(StatRule {
        bonus: Some(bonus.to_string()),
        ..stat(name, value)
    })
}

/// `stat` rule active from `level`.
#[must_use]
pub fn stat_at(name: &str, value: impl ToString, level: u32) -> Rule {
    Rule::Stat(StatRule {
        level: (level > 1).then_some(level),
        ..stat(name, value)
    })
}

/// `select` rule.
#[must_use]
pub fn select(ty: &str, name: &str, supports: Option<&str>, number: u32) -> SelectRule {
    SelectRule {
        ty: ty.to_string(),
        name: name.to_string(),
        number: (number > 1).then_some(number),
        supports: supports.map(str::to_string),
        ..SelectRule::default()
    }
}

/// A setter with attributes.
#[must_use]
pub fn setter_with(name: &str, value: &str, attrs: &[(&str, &str)]) -> Setter {
    let mut s = Setter::new(name, value);
    for (k, v) in attrs {
        s.attributes.insert((*k).to_string(), (*v).to_string());
    }
    s
}

/// Why a gate or level could not be applied: the engine reads no such field on this rule kind,
/// so applying it silently would drop the condition.
fn unsupported(rule: &Rule, field: &str) -> String {
    let kind = match rule {
        Rule::Custom(c) => c.kind.as_str(),
        _ => rule.kind().as_str(),
    };
    format!("rule kind {kind:?} has no `{field}` the engine reads; the {field} was not applied")
}

/// Does the engine read `field` on this custom rule (`rule_kinds::KNOWN_KINDS`)?
fn custom_reads(c: &borealis_content::CustomRule, field: &str) -> bool {
    borealis_content::rule_kinds::spec(&c.kind).is_some_and(|s| s.reads(field))
}

/// Add `requirements` to a rule (joined with `&&` when it has some already).
///
/// # Errors
/// The rule kind has no `requirements` the engine reads (an unknown rule, or a custom kind
/// whose reader ignores it); the rule is left unchanged.
pub fn gate(rule: &mut Rule, requirements: &str) -> Result<(), String> {
    let join = |existing: Option<&str>| match existing {
        Some(e) if !e.is_empty() => format!("({e})&&{requirements}"),
        _ => requirements.to_string(),
    };
    let slot = match rule {
        Rule::Grant(r) => &mut r.requirements,
        Rule::Select(r) => &mut r.requirements,
        Rule::Stat(r) => &mut r.requirements,
        Rule::Custom(c) if custom_reads(c, "requirements") => {
            let joined = join(c.fields.get("requirements").and_then(|v| v.as_str()));
            c.fields
                .insert("requirements".into(), serde_json::Value::String(joined));
            return Ok(());
        }
        Rule::Unknown(_) | Rule::Custom(_) => return Err(unsupported(rule, "requirements")),
    };
    *slot = Some(join(slot.as_deref()));
    Ok(())
}

/// Set the rule level (`None` at level 1).
///
/// # Errors
/// The rule kind has no `level` the engine reads; the rule is left unchanged.
pub fn set_level(rule: &mut Rule, level: u32) -> Result<(), String> {
    let value = (level > 1).then_some(level);
    let slot = match rule {
        Rule::Grant(r) => &mut r.level,
        Rule::Select(r) => &mut r.level,
        Rule::Stat(r) => &mut r.level,
        Rule::Custom(c) if custom_reads(c, "level") => {
            match value {
                Some(l) => c.fields.insert("level".into(), l.into()),
                None => c.fields.remove("level"),
            };
            return Ok(());
        }
        Rule::Unknown(_) | Rule::Custom(_) => return Err(unsupported(rule, "level")),
    };
    *slot = value;
    Ok(())
}

/// A custom (forward compatible) rule.
#[must_use]
pub fn custom(kind: &str, fields: serde_json::Value) -> Rule {
    let serde_json::Value::Object(fields) = fields else {
        unreachable!("custom rule fields must be an object");
    };
    Rule::Custom(borealis_content::CustomRule {
        kind: kind.to_string(),
        fields,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn constructors_and_gates() {
        let mut g = grant("ID_X", "Proficiency");
        gate(&mut g, "!ID_MC").unwrap();
        assert_eq!(g.requirements(), Some("!ID_MC"));
        gate(&mut g, "ID_OTHER").unwrap();
        assert_eq!(g.requirements(), Some("(!ID_MC)&&ID_OTHER"));
        let mut s = Rule::Select(select("Language", "L", Some("Standard"), 2));
        set_level(&mut s, 3).unwrap();
        assert_eq!(s.level(), 3);
        set_level(&mut s, 1).unwrap();
        assert_eq!(s.level(), 1);
        let Rule::Select(sel) = &s else {
            unreachable!()
        };
        assert_eq!(sel.number, Some(2));
        assert_eq!(grant_at("ID_Y", "Spell", 1).level(), 1);
        assert_eq!(grant_at("ID_Y", "Spell", 5).level(), 5);
        assert_eq!(stat_at("hp", 1, 4).level(), 4);
        let Rule::Stat(st) = stat_bonus("a", -2, "base") else {
            unreachable!()
        };
        assert_eq!(st.value, "-2");
        let sw = setter_with("n", "v", &[("k", "x")]);
        assert_eq!(sw.attribute("k"), Some("x"));
        let c = custom("grant.x", json!({"a": 1}));
        assert_eq!(c.kind().as_str(), "custom");
        // an unknown kind is refused loudly and left unchanged
        let mut c2 = c.clone();
        assert!(gate(&mut c2, "x").unwrap_err().contains("grant.x"));
        assert!(set_level(&mut c2, 2).is_err());
        assert_eq!(c2, c);
        // a known kind whose reader ignores the field is refused too
        let mut book = custom("spellbook", json!({"initial_spells": 6}));
        assert!(gate(&mut book, "ID_A").is_err());
        assert!(set_level(&mut book, 2).is_err());
        let mut gs = custom("grant_spell", json!({"spell": "ID_S"}));
        gate(&mut gs, "!ID_A").unwrap();
        gate(&mut gs, "ID_B").unwrap();
        set_level(&mut gs, 3).unwrap();
        let Rule::Custom(g) = &gs else { unreachable!() };
        assert_eq!(g.fields["requirements"], "(!ID_A)&&ID_B");
        assert_eq!(g.fields["level"], 3);
        set_level(&mut gs, 1).unwrap();
        let Rule::Custom(g) = &gs else { unreachable!() };
        assert!(!g.fields.contains_key("level"));
    }
}
