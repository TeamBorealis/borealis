//! Field-by-field comparison of what Aurora wrote with what Borealis computes, plus the reviewed
//! exception list (`exceptions.tsv` in the private results directory).

use super::sheet::{damage_key, Casting, Sheet};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// One compared value. `id` is `<field path>` within the file, `field` the rate group.
#[derive(Debug, Clone)]
pub struct Check {
    pub id: String,
    pub field: &'static str,
    pub ok: bool,
    pub expected: String,
    pub actual: String,
}

fn push(out: &mut Vec<Check>, id: String, field: &'static str, e: String, a: String) {
    out.push(Check {
        ok: e == a,
        id,
        field,
        expected: e,
        actual: a,
    });
}

fn list(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(",")
}

/// Exact set comparison; on a mismatch the strings name only the differing ids.
fn set_check(
    out: &mut Vec<Check>,
    id: String,
    field: &'static str,
    e: &BTreeSet<String>,
    a: &BTreeSet<String>,
) {
    if e == a {
        push(
            out,
            id,
            field,
            format!("{} ids", e.len()),
            format!("{} ids", a.len()),
        );
        return;
    }
    let missing: BTreeSet<_> = e.difference(a).cloned().collect();
    let extra: BTreeSet<_> = a.difference(e).cloned().collect();
    out.push(Check {
        id,
        field,
        ok: false,
        expected: format!("{} ids; Borealis lacks [{}]", e.len(), list(&missing)),
        actual: format!("{} ids; Borealis adds [{}]", a.len(), list(&extra)),
    });
}

fn casting_checks(out: &mut Vec<Check>, name: &str, e: &Casting, a: Option<&Casting>) {
    let p = format!("casting/{name}");
    let none = Casting::default();
    let absent = a.is_none();
    let a = a.unwrap_or(&none);
    let val = |v: String| if absent { "(no block)".to_string() } else { v };
    push(
        out,
        format!("{p}/ability"),
        "casting.ability",
        e.ability.to_lowercase(),
        val(a.ability.to_lowercase()),
    );
    push(
        out,
        format!("{p}/attack"),
        "casting.attack",
        e.attack.to_string(),
        val(a.attack.to_string()),
    );
    push(
        out,
        format!("{p}/dc"),
        "casting.dc",
        e.dc.to_string(),
        val(a.dc.to_string()),
    );
    push(
        out,
        format!("{p}/slots"),
        "casting.slots",
        format!("{:?}", e.slots),
        val(format!("{:?}", a.slots)),
    );
    set_check(
        out,
        format!("{p}/cantrips"),
        "casting.cantrips",
        &e.cantrips,
        &a.cantrips,
    );
    set_check(
        out,
        format!("{p}/prepared"),
        "casting.prepared",
        &e.prepared,
        &a.prepared,
    );
    set_check(
        out,
        format!("{p}/known"),
        "casting.known",
        &e.known,
        &a.known,
    );
    // The full list of a prepare-from-list caster depends on every pack loaded when Aurora saved
    // the file: every listed spell must be listed by Borealis; extra spells are informational.
    let missing: BTreeSet<_> = e.list.difference(&a.list).cloned().collect();
    let extra = a.list.difference(&e.list).count();
    out.push(Check {
        id: format!("{p}/list"),
        field: "casting.list",
        ok: missing.is_empty(),
        expected: format!("{} ids; Borealis lacks [{}]", e.list.len(), list(&missing)),
        actual: format!("{} ids ({extra} more than Aurora)", a.list.len()),
    });
}

/// Every check for one character.
pub fn compare(e: &Sheet, a: &Sheet) -> Vec<Check> {
    let mut out = Vec::new();
    push(
        &mut out,
        "display/level".into(),
        "level",
        e.level.to_string(),
        a.level.to_string(),
    );
    push(
        &mut out,
        "display/race".into(),
        "race",
        e.race.clone(),
        a.race.clone(),
    );
    push(
        &mut out,
        "display/class".into(),
        "class",
        e.class.clone(),
        a.class.clone(),
    );
    push(
        &mut out,
        "display/background".into(),
        "background",
        e.background.clone(),
        a.background.clone(),
    );
    push(
        &mut out,
        "abilities/base".into(),
        "ability_base",
        format!("{:?}", e.base),
        format!("{:?}", a.base),
    );
    for (name, c) in &e.casting {
        casting_checks(&mut out, name, c, a.casting.get(name));
    }
    let extra: BTreeSet<_> = a
        .casting
        .keys()
        .filter(|k| !e.casting.contains_key(*k))
        .cloned()
        .collect();
    push(
        &mut out,
        "casting/extra_blocks".into(),
        "casting.blocks",
        String::new(),
        list(&extra),
    );
    set_check(
        &mut out,
        "magic/additional".into(),
        "additional_spells",
        &e.additional,
        &a.additional,
    );
    let by_uid: BTreeMap<String, &super::sheet::Attack> = a
        .attacks
        .iter()
        .map(|x| (x.uid.to_lowercase(), x))
        .collect();
    for (i, at) in e.attacks.iter().enumerate() {
        let p = format!("attack/{}:{}", i + 1, at.name);
        let got = by_uid.get(&at.uid.to_lowercase());
        // Aurora recomputes every item-linked attack row, equipped or not; Borealis shows rows
        // for equipped weapons only (the values are still compared, see `Attack::shown`).
        push(
            &mut out,
            format!("{p}/row"),
            "attack.row",
            "shown".into(),
            got.map_or_else(
                || "(no attack)".into(),
                |g| {
                    if g.shown {
                        "shown".into()
                    } else {
                        "not shown (weapon not equipped)".into()
                    }
                },
            ),
        );
        let bonus = |b: Option<i32>| b.map_or_else(|| "-".to_string(), |v| format!("{v:+}"));
        push(
            &mut out,
            format!("{p}/attack"),
            "attack.bonus",
            bonus(at.bonus),
            got.map_or_else(|| "(no attack)".into(), |g| bonus(g.bonus)),
        );
        let ek = damage_key(&at.damage);
        let ak = got.map(|g| damage_key(&g.damage));
        out.push(Check {
            id: format!("{p}/damage"),
            field: "attack.damage",
            ok: ak.as_ref() == Some(&ek),
            expected: at.damage.clone(),
            actual: got.map_or_else(|| "(no attack)".into(), |g| g.damage.clone()),
        });
    }
    out
}

/// A reviewed exception: `(file key, check id) -> (class, reason)`.
pub type Exceptions = BTreeMap<(String, String), (String, String)>;

/// `exceptions.tsv`: `file<TAB>check<TAB>class (b|c)<TAB>reason`; `#` lines are comments.
pub fn load_exceptions(path: &Path) -> Exceptions {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Exceptions::new();
    };
    let mut out = Exceptions::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.splitn(4, '\t').collect();
        assert!(
            cols.len() == 4 && matches!(cols[2], "b" | "c") && !cols[3].trim().is_empty(),
            "{}:{}: want file<TAB>check<TAB>b|c<TAB>reason",
            path.display(),
            n + 1
        );
        out.insert(
            (cols[0].to_string(), cols[1].to_string()),
            (cols[2].to_string(), cols[3].to_string()),
        );
    }
    out
}
