//! The values an Aurora `.dnd5e` file carries that Aurora computed when it saved the character,
//! read from the file (`from_file`) and the same values computed by Borealis (`from_snapshot`).
//!
//! Aurora does not write HP, AC, initiative, speed, proficiency bonus, final ability scores or
//! the character's save and skill totals (character-file-format.md section 5.2); the companion
//! block's saves and skills belong to the companion. What it does write, and what is compared:
//! the display header (level, race, class, background), the base ability scores, every
//! `magic/spellcasting` block (ability, attack, DC, slots, cantrips, the spell list and its
//! prepared and known flags), the `magic/additional` spells, every item-linked
//! `attack` (attack bonus and damage) and the `sum` manifest (a coverage signal only).

use borealis_content::ContentStore;
use borealis_engine::CharacterSnapshot;
use borealis_migrate::dom::XmlElement;
use std::collections::{BTreeMap, BTreeSet};

pub const ABILITIES: [&str; 6] = [
    "strength",
    "dexterity",
    "constitution",
    "intelligence",
    "wisdom",
    "charisma",
];

#[derive(Debug, Default, Clone)]
pub struct Casting {
    pub ability: String,
    pub attack: i32,
    pub dc: i32,
    pub slots: [i32; 9],
    pub cantrips: BTreeSet<String>,
    pub list: BTreeSet<String>,
    pub prepared: BTreeSet<String>,
    pub known: BTreeSet<String>,
}

#[derive(Debug, Default, Clone)]
pub struct Attack {
    pub uid: String,
    pub name: String,
    pub bonus: Option<i32>,
    pub damage: String,
    /// Borealis side: the weapon is equipped, so the sheet shows this attack row. `false` =
    /// the values come from a copy of the character with the weapon put in the main hand.
    pub shown: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Sheet {
    pub level: u32,
    pub race: String,
    pub class: String,
    pub background: String,
    pub base: [i32; 6],
    pub casting: BTreeMap<String, Casting>,
    pub additional: BTreeSet<String>,
    /// Item-linked attacks (manual attacks are free text and are not compared).
    /// An attack whose item is no longer in the inventory counts as manual.
    pub attacks: Vec<Attack>,
    pub manual_attacks: usize,
    pub sum_count: usize,
    pub sum_ids: BTreeSet<String>,
    pub companion: String,
}

fn child_text(e: &XmlElement, path: &[&str]) -> String {
    let mut cur = e;
    for p in path {
        match cur.child(p) {
            Some(c) => cur = c,
            None => return String::new(),
        }
    }
    cur.text().trim().to_string()
}

fn int(s: Option<&str>) -> i32 {
    s.and_then(|v| v.trim().trim_start_matches('+').parse().ok())
        .unwrap_or(0)
}

fn kids<'a>(e: Option<&'a XmlElement>, name: &'a str) -> impl Iterator<Item = &'a XmlElement> {
    e.into_iter()
        .flat_map(move |x| x.elements().filter(move |c| c.name == name))
}

fn flag(e: &XmlElement, name: &str) -> bool {
    e.attr(name).is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

/// `"+8 vs AC"` -> 8.
fn attack_bonus(text: &str) -> Option<i32> {
    let t = text.trim();
    let num: String = t
        .chars()
        .take_while(|c| *c == '+' || *c == '-' || c.is_ascii_digit())
        .collect();
    num.trim_start_matches('+').parse().ok()
}

fn read_casting(sc: &XmlElement) -> Casting {
    let mut c = Casting {
        ability: sc.attr("ability").unwrap_or("").to_string(),
        attack: int(sc.attr("attack")),
        dc: int(sc.attr("dc")),
        ..Casting::default()
    };
    if let Some(sl) = sc.child("slots") {
        for (i, s) in c.slots.iter_mut().enumerate() {
            *s = int(sl.attr(&format!("s{}", i + 1)));
        }
    }
    for sp in kids(sc.child("cantrips"), "spell") {
        c.cantrips.insert(sp.attr("id").unwrap_or("").to_string());
    }
    for sp in kids(sc.child("spells"), "spell") {
        let id = sp.attr("id").unwrap_or("").to_string();
        if sp.attr("level") == Some("0") {
            c.cantrips.insert(id);
            continue;
        }
        c.list.insert(id.clone());
        // Aurora writes `always-prepared` for every registered spell of a preparing class, the
        // same set as `known` (not the always-prepared flag of the grant), so only `prepared`
        // (chosen or granted-prepared) and `known` carry information.
        if flag(sp, "prepared") {
            c.prepared.insert(id.clone());
        }
        if flag(sp, "known") {
            c.known.insert(id);
        }
    }
    c
}

/// The values Aurora wrote into the file.
pub fn from_file(root: &XmlElement) -> Sheet {
    let mut s = Sheet::default();
    let dp = root.child("display-properties");
    if let Some(dp) = dp {
        s.level = child_text(dp, &["level"]).parse().unwrap_or(0);
        s.race = child_text(dp, &["race"]);
        s.class = child_text(dp, &["class"]);
        s.background = child_text(dp, &["background"]);
    }
    let Some(build) = root.child("build") else {
        return s;
    };
    if let Some(ab) = build.child("abilities") {
        for (i, name) in ABILITIES.iter().enumerate() {
            let v = ab
                .elements()
                .find(|e| e.name.eq_ignore_ascii_case(name))
                .map(XmlElement::text);
            s.base[i] = int(v.as_deref());
        }
    }
    let items: BTreeSet<String> = kids(build.child("equipment"), "item")
        .filter_map(|i| i.attr("identifier"))
        .map(|u| u.trim().to_lowercase())
        .collect();
    let attacks = build.child("input").and_then(|i| i.child("attacks"));
    for a in kids(attacks, "attack") {
        let uid = a.attr("identifier").unwrap_or("").trim().to_string();
        // Aurora's loader turns an attack whose item is gone into a manual attack.
        if uid.is_empty() || !items.contains(&uid.to_lowercase()) {
            s.manual_attacks += 1;
            continue;
        }
        s.attacks.push(Attack {
            uid,
            name: a.attr("name").unwrap_or("").to_string(),
            bonus: attack_bonus(a.attr("attack").unwrap_or("")),
            damage: a.attr("damage").unwrap_or("").to_string(),
            shown: true,
        });
    }
    let magic = build.child("magic");
    for sc in kids(magic, "spellcasting") {
        let name = sc.attr("name").unwrap_or("").to_string();
        s.casting.insert(name, read_casting(sc));
    }
    for sp in kids(magic.and_then(|m| m.child("additional")), "spell") {
        s.additional.insert(sp.attr("id").unwrap_or("").to_string());
    }
    if let Some(sum) = build.child("sum") {
        s.sum_count = int(sum.attr("element-count")).try_into().unwrap_or(0);
        for e in kids(Some(sum), "element") {
            s.sum_ids.insert(e.attr("id").unwrap_or("").to_string());
        }
    }
    s.companion = build
        .child("companion")
        .and_then(|c| c.attr("name"))
        .unwrap_or("")
        .trim()
        .to_string();
    s
}

/// Aurora's display name of an element: the sheet's alternate name, else the name.
fn alt_name(store: &ContentStore, id: &str, name: &str) -> String {
    store
        .element(id)
        .and_then(|e| e.sheet.as_ref())
        .and_then(|sh| sh.alt.clone())
        .unwrap_or_else(|| name.to_string())
}

/// First registered element of a type: `(id, name)`.
fn first_of<'a>(snap: &'a CharacterSnapshot, ty: &str) -> Option<(&'a str, &'a str)> {
    snap.elements
        .iter()
        .find(|r| r.element.ty == ty)
        .map(|r| (r.element.id.as_str(), r.element.name.as_str()))
}

/// The display header the way Aurora builds it: race = the sub race's (else the race's)
/// alternate name; background = the background variant's alternate name, else the background's
/// name; class = the class's alternate name, `Name (level) / Other (level)` when multiclassed.
fn header(snap: &CharacterSnapshot, store: &ContentStore, s: &mut Sheet) {
    s.race = first_of(snap, "Sub Race")
        .or_else(|| first_of(snap, "Race"))
        .map(|(id, n)| alt_name(store, id, n))
        .unwrap_or_default();
    s.background = first_of(snap, "Background Variant")
        .map(|(id, n)| alt_name(store, id, n))
        .or_else(|| first_of(snap, "Background").map(|(_, n)| n.to_string()))
        .unwrap_or_default();
    let named: Vec<_> = snap
        .classes
        .iter()
        .filter_map(|c| {
            let e = c.class.as_ref()?;
            Some((alt_name(store, &e.id, &e.name), c.level))
        })
        .collect();
    s.class = if named.len() == 1 {
        named[0].0.clone()
    } else {
        named
            .iter()
            .map(|(n, l)| format!("{n} ({l})"))
            .collect::<Vec<_>>()
            .join(" / ")
    };
}

/// The same values computed by Borealis.
pub fn from_snapshot(snap: &CharacterSnapshot, store: &ContentStore) -> Sheet {
    let mut s = Sheet {
        level: snap.level,
        ..Sheet::default()
    };
    header(snap, store, &mut s);
    for (i, name) in ABILITIES.iter().enumerate() {
        s.base[i] = snap.ability(name).map_or(0, |a| a.base);
    }
    let mut in_blocks = BTreeSet::new();
    for sc in &snap.spellcasting {
        let mut c = Casting {
            ability: sc.ability.clone(),
            attack: sc.attack,
            dc: sc.dc,
            slots: sc.slots,
            ..Casting::default()
        };
        if let Some((n, lvl)) = sc.pact {
            if let Some(slot) = usize::try_from(lvl - 1)
                .ok()
                .and_then(|i| c.slots.get_mut(i))
            {
                *slot += n;
            }
        }
        for sp in &sc.spells {
            in_blocks.insert(sp.id.clone());
            if sp.level == 0 {
                c.cantrips.insert(sp.id.clone());
                continue;
            }
            c.list.insert(sp.id.clone());
            if sp.prepared {
                c.prepared.insert(sp.id.clone());
            }
            if sp.known {
                c.known.insert(sp.id.clone());
            }
        }
        s.casting.insert(sc.name.clone(), c);
    }
    s.additional = snap
        .elements
        .iter()
        .filter(|r| r.element.ty == "Spell" && !in_blocks.contains(&r.element.id))
        .map(|r| r.element.id.clone())
        .collect();
    s.attacks = snap
        .attacks
        .iter()
        .map(|a| Attack {
            uid: a.uid.clone(),
            name: a.name.clone(),
            bonus: Some(a.attack),
            damage: a.damage.clone(),
            shown: true,
        })
        .collect();
    s.sum_ids = snap.elements.iter().map(|r| r.element.id.clone()).collect();
    s.sum_count = snap.elements.len();
    s
}

/// Damage text in a comparable form: `(dice, modifier, damage type)`, spacing and case ignored,
/// a missing modifier = 0 (`1d6+0 piercing` == `1d6 piercing`).
pub fn damage_key(text: &str) -> (String, i32, String) {
    let t = text.trim().to_lowercase();
    let compact: String = {
        let mut out = String::new();
        let mut chars = t.chars().peekable();
        while let Some(c) = chars.next() {
            if c == ' ' {
                let prev = out.chars().last();
                let next = chars.peek().copied();
                if matches!(prev, Some('+' | '-')) || matches!(next, Some('+' | '-')) {
                    continue;
                }
            }
            out.push(c);
        }
        out
    };
    let (expr, ty) = compact
        .split_once(' ')
        .map_or((compact.as_str(), ""), |(a, b)| (a, b.trim()));
    let cut = expr
        .char_indices()
        .skip(1)
        .filter(|(_, c)| *c == '+' || *c == '-')
        .map(|(i, _)| i)
        .last();
    let (dice, m) = match cut {
        Some(i) => (
            &expr[..i],
            expr[i..].trim_start_matches('+').parse().unwrap_or(0),
        ),
        None => (expr, 0),
    };
    let dice = dice.trim_matches(|c| c == '\u{2014}' || c == '-' || c == '+');
    (dice.to_string(), m, ty.to_string())
}
