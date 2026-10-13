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

/// Set the `requirements` of a rule when it has none yet.
pub fn gate(rule: &mut Rule, requirements: &str) {
    let slot = match rule {
        Rule::Grant(r) => &mut r.requirements,
        Rule::Select(r) => &mut r.requirements,
        Rule::Stat(r) => &mut r.requirements,
        Rule::Unknown(_) | Rule::Custom(_) => return,
    };
    match slot {
        Some(existing) if !existing.is_empty() => {
            *slot = Some(format!("({existing})&&{requirements}"));
        }
        _ => *slot = Some(requirements.to_string()),
    }
}

/// Set the rule level when the rule kind has one.
pub fn set_level(rule: &mut Rule, level: u32) {
    let slot = match rule {
        Rule::Grant(r) => &mut r.level,
        Rule::Select(r) => &mut r.level,
        Rule::Stat(r) => &mut r.level,
        Rule::Unknown(_) | Rule::Custom(_) => return,
    };
    *slot = (level > 1).then_some(level);
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
        gate(&mut g, "!ID_MC");
        assert_eq!(g.requirements(), Some("!ID_MC"));
        gate(&mut g, "ID_OTHER");
        assert_eq!(g.requirements(), Some("(!ID_MC)&&ID_OTHER"));
        let mut s = Rule::Select(select("Language", "L", Some("Standard"), 2));
        set_level(&mut s, 3);
        assert_eq!(s.level(), 3);
        set_level(&mut s, 1);
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
        let mut c2 = c.clone();
        gate(&mut c2, "x");
        set_level(&mut c2, 2);
        assert_eq!(c2, c);
    }
}
