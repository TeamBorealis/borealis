//! Header, free-text, ability, source and magic sections.

use super::{Ctx, MissingContent};
use crate::model::{Character, FeatureText, ManualAttack, Portrait};
use borealis_engine::state::{AbilityMethod, PreparedSpells};
use borealis_migrate::dom::XmlElement;

pub(super) fn norm(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Trimmed inner text of a named child, `\r\n` normalised.
pub(super) fn text_of(el: &XmlElement, child: &str) -> String {
    el.child(child)
        .map(|c| norm(&c.text()).trim().to_string())
        .unwrap_or_default()
}

fn raw_text_of(el: &XmlElement, child: &str) -> String {
    el.child(child).map(|c| norm(&c.text())).unwrap_or_default()
}

fn parse_bool(s: Option<&str>) -> bool {
    s.is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
}

pub(super) fn read_header(root: &XmlElement, c: &mut Character, ctx: &mut Ctx<'_>) {
    if let Some(info) = root.child("information") {
        c.meta.group = text_of(info, "group");
        ctx.state.abilities.method = match text_of(info, "generationOption").as_str() {
            "0" => AbilityMethod::Roll3d6,
            "2" => AbilityMethod::StandardArray,
            "3" => AbilityMethod::PointBuy,
            "1" => AbilityMethod::Roll4d6,
            _ => ctx.state.abilities.method,
        };
    }
    if c.meta.group.is_empty() {
        c.meta.group = "Characters".into();
    }
    if let Some(dp) = root.child("display-properties") {
        c.meta.favorite = parse_bool(dp.attr("favorite"));
        c.meta.name = text_of(dp, "name");
        if let Some(b64) = dp
            .child("portrait")
            .and_then(|p| p.child("base64"))
            .map(|b| b.text().trim().to_string())
            .filter(|b| !b.is_empty())
        {
            let local = dp
                .child("portrait")
                .map(|p| text_of(p, "local"))
                .unwrap_or_default();
            let file_name = local
                .rsplit(['\\', '/'])
                .next()
                .filter(|n| !n.is_empty())
                .unwrap_or("portrait.png")
                .to_lowercase();
            c.portrait = Some(Portrait {
                file_name,
                data_base64: b64,
            });
        }
    }
}

pub(super) fn read_input(build: &XmlElement, c: &mut Character, ctx: &mut Ctx<'_>) {
    let Some(input) = build.child("input") else {
        ctx.report
            .warnings
            .push("the file has no <input> section".into());
        return;
    };
    let name = text_of(input, "name");
    if !name.is_empty() {
        c.meta.name = name;
    }
    c.meta.gender = text_of(input, "gender");
    c.meta.player = text_of(input, "player-name");
    ctx.state.experience = text_of(input, "experience").parse().unwrap_or(0);
    let d = &mut c.details;
    d.backstory = raw_text_of(input, "backstory");
    d.personality.trinket = text_of(input, "background-trinket");
    d.personality.traits = text_of(input, "background-traits");
    d.personality.ideals = text_of(input, "background-ideals");
    d.personality.bonds = text_of(input, "background-bonds");
    d.personality.flaws = text_of(input, "background-flaws");
    if let Some(f) = input.child("background").and_then(|b| b.child("feature")) {
        d.background_feature = Some(FeatureText {
            name: f.attr("name").unwrap_or("").to_string(),
            description: raw_text_of(f, "description"),
        });
    }
    if let Some(o) = input.child("organization") {
        d.organization.name = text_of(o, "name");
        d.organization.symbol = text_of(o, "symbol");
        d.organization.allies = raw_text_of(o, "allies");
    }
    d.additional_features = raw_text_of(input, "additional-features");
    d.quest = text_of(input, "quest");
    if let Some(cur) = input.child("currency") {
        let n = |k: &str| text_of(cur, k).parse::<u64>().unwrap_or(0);
        ctx.state.inventory.coins = borealis_engine::state::Coins {
            cp: n("copper"),
            sp: n("silver"),
            ep: n("electrum"),
            gp: n("gold"),
            pp: n("platinum"),
        };
        d.equipment_notes = raw_text_of(cur, "equipment");
        d.treasure = raw_text_of(cur, "treasure");
    }
    if let Some(notes) = input.child("notes") {
        for n in notes.elements().filter(|e| e.name == "note") {
            let t = norm(&n.text());
            match n.attr("column") {
                Some("right") => d.notes.right = t,
                _ => d.notes.left = t,
            }
        }
    }
    if let Some(att) = input.child("attacks") {
        d.attacks_text = raw_text_of(att, "description");
        for a in att.elements().filter(|e| e.name == "attack") {
            d.attacks.push(ManualAttack {
                item_uid: a
                    .attr("identifier")
                    .filter(|i| !i.is_empty())
                    .map(str::to_lowercase),
                name: a.attr("name").unwrap_or("").to_string(),
                range: a.attr("range").unwrap_or("").to_string(),
                attack: a.attr("attack").unwrap_or("").to_string(),
                damage: a.attr("damage").unwrap_or("").to_string(),
                displayed: parse_bool(a.attr("displayed")),
                ability: a.attr("ability").unwrap_or("").to_string(),
                description: raw_text_of(a, "description"),
                as_card: a.attr("card").map(|v| parse_bool(Some(v))),
            });
        }
    }
    if let Some(ap) = build.child("appearance") {
        d.appearance.age = text_of(ap, "age");
        d.appearance.height = text_of(ap, "height");
        d.appearance.weight = text_of(ap, "weight");
        d.appearance.eyes = text_of(ap, "eyes");
        d.appearance.skin = text_of(ap, "skin");
        d.appearance.hair = text_of(ap, "hair");
    }
    if let Some(def) = build.child("defenses") {
        d.conditional_ac = text_of(def, "conditional");
    }
    if let Some(comp) = build.child("companion") {
        d.companion_name = comp.attr("name").unwrap_or("").to_string();
    }
}

pub(super) fn read_abilities(build: &XmlElement, ctx: &mut Ctx<'_>) {
    let Some(ab) = build.child("abilities") else {
        ctx.report
            .warnings
            .push("the file has no <abilities> section; scores default to 10".into());
        return;
    };
    for (i, name) in borealis_engine::ids::ABILITIES.iter().enumerate() {
        let node = ab.elements().find(|e| e.name.eq_ignore_ascii_case(name));
        match node.and_then(|n| n.text().trim().parse::<i32>().ok()) {
            Some(v) => ctx.state.abilities.base[i] = v,
            None => ctx
                .report
                .warnings
                .push(format!("missing or invalid base {name} score")),
        }
    }
}

pub(super) fn read_sources(root: &XmlElement, ctx: &mut Ctx<'_>) {
    let Some(restricted) = root.child("sources").and_then(|s| s.child("restricted")) else {
        return;
    };
    for s in restricted.elements().filter(|e| e.name == "source") {
        let id = s.attr("id").unwrap_or("");
        let display = s.text().trim().to_string();
        let name = ctx
            .uni
            .get(id)
            .filter(|e| e.el.ty == "Source")
            .map_or(display, |e| e.el.name.clone());
        if !name.is_empty() && !ctx.state.restricted_sources.contains(&name) {
            ctx.state.restricted_sources.push(name);
        }
    }
}

pub(super) fn read_magic(build: &XmlElement, ctx: &mut Ctx<'_>) {
    let Some(magic) = build.child("magic") else {
        return;
    };
    for sc in magic.elements().filter(|e| e.name == "spellcasting") {
        let class = sc.attr("name").unwrap_or("").to_string();
        let mut spells = Vec::new();
        for grp in sc
            .elements()
            .filter(|e| e.name == "cantrips" || e.name == "spells")
        {
            for sp in grp.elements().filter(|e| e.name == "spell") {
                if parse_bool(sp.attr("prepared")) && !parse_bool(sp.attr("always-prepared")) {
                    if let Some(id) = sp.attr("id") {
                        spells.push(id.to_string());
                    }
                }
            }
        }
        if !class.is_empty() && !spells.is_empty() {
            ctx.state.prepared.push(PreparedSpells { class, spells });
        }
    }
}

/// Use `sum` (the flat list of every registered id) to report content the install lacks.
pub(super) fn read_dependencies(build: &XmlElement, ctx: &mut Ctx<'_>) {
    let Some(sum) = build.child("sum") else {
        return;
    };
    for e in sum.elements().filter(|e| e.name == "element") {
        let id = e.attr("id").unwrap_or("");
        if id.is_empty()
            || ctx.uni.get(id).is_some()
            || ctx.report.missing_content.iter().any(|m| m.id == id)
        {
            continue;
        }
        ctx.report.missing_content.push(MissingContent {
            id: id.to_string(),
            ty: e.attr("type").unwrap_or("").to_string(),
            name: e.attr("name").unwrap_or("").to_string(),
            context: "dependency".into(),
        });
    }
}
