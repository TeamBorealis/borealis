//! Rows for the classic builder's generated proxy items (`specs/proxy-items.md` P8, IM13): the
//! equipment screen's "Additional ..." categories of an edition, adding and equipping one proxy
//! of each kind, and the sheet leaving its hidden line out. A proxy is never an offering host
//! (IM13 D5), so these rows are separate from the stage-2 offer rows. Ids and labels only.

use super::app::{Driver, TEST_SCORES};
use crate::{outcome, SUITE};
use borealis_content::{Rule, Ruleset};
use borealis_lib::dto_ext::{EquipRequest, ItemFilter, SheetOptionsDto};
use borealis_lib::state::AppState;

/// The generated categories every edition lists (their targets include the core catalogue).
const KINDS: [(&str, &str); 5] = [
    ("feat", "Feat"),
    ("language", "Language"),
    ("proficiency", "Proficiency"),
    ("asi", "Ability Score Improvement"),
    ("spell", "Spell"),
];

fn generated(ty: &str) -> String {
    format!("Additional {ty}")
}

/// Record the proxy rows of one edition on a fresh character (its own driver, so the stage-2
/// host counts stay as they were).
pub fn record(src: &str, app: &AppState, rs: Ruleset) {
    let d = Driver::new(app);
    let Some(host) = d.create(rs, TEST_SCORES) else {
        return;
    };
    let edition = format!("{rs:?}").to_lowercase();
    categories_row(src, app, &host.id, &edition);
    let mut sheet_ok = None;
    for (kind, ty) in KINDS {
        let ok = add_row(app, &host.id, &generated(ty));
        if kind == "feat" {
            sheet_ok = Some(ok && sheet_leaves_out_hidden_lines(app, &host.id));
        }
        outcome::record(
            SUITE,
            &format!("{src}/ui/equipment/proxy-add/{kind}"),
            ok,
            "",
        );
    }
    let cast = cast_category(app, rs);
    let ok = cast.as_deref().is_some_and(|c| add_row(app, &host.id, c));
    outcome::record(
        SUITE,
        &format!("{src}/ui/equipment/proxy-add/cast-spell"),
        ok,
        if cast.is_none() {
            "no spellcasting name"
        } else {
            ""
        },
    );
    outcome::record(
        SUITE,
        &format!("{src}/sheet/hidden-line"),
        sheet_ok == Some(true),
        "",
    );
    d.drop_host(host);
}

/// Every generated category is listed with the count its item search returns.
fn categories_row(src: &str, app: &AppState, id: &str, edition: &str) {
    let list = app.list_item_categories(id).unwrap_or_default();
    let mut bad: Vec<String> = Vec::new();
    for (_, ty) in KINDS {
        let name = generated(ty);
        let Some(c) = list.iter().find(|c| c.name == name) else {
            bad.push(format!("{ty}: absent"));
            continue;
        };
        let total = search(app, id, &name, 1).map_or(0, |p| p.0);
        if total != c.count {
            bad.push(format!("{ty}: count {} != rows {total}", c.count));
        }
    }
    outcome::record(
        SUITE,
        &format!("{src}/ui/equipment/proxy-categories/{edition}"),
        bad.is_empty(),
        &bad.join("; "),
    );
}

/// `(total, first row id, first row name)` of a category search.
fn search(
    app: &AppState,
    id: &str,
    category: &str,
    limit: usize,
) -> Option<(usize, String, String)> {
    let page = app
        .search_items(&ItemFilter {
            category: Some(category.to_string()),
            query: None,
            source: None,
            limit: Some(limit),
            offset: None,
            character_id: Some(id.to_string()),
        })
        .ok()?;
    let first = &page.items.first()?.summary;
    Some((page.total, first.id.clone(), first.name.clone()))
}

/// Add the first proxy of `category`, equip it: its target is registered; unequip and remove it.
fn add_row(app: &AppState, id: &str, category: &str) -> bool {
    let Some((_, proxy, _)) = search(app, id, category, 1) else {
        return false;
    };
    let target = granted(app, id, &proxy);
    if app.add_item(id, &proxy, 1, false, None).is_err() {
        return false;
    }
    let Some(uid) = last_line(app, id) else {
        return false;
    };
    let equipped = app.equip_item(id, &uid, EquipRequest::Auto).is_ok();
    let has = target.is_some_and(|t| registered(app, id, &t));
    let _ = app.remove_item(id, &uid, true);
    equipped && has
}

/// What the proxy grants (its one grant rule).
fn granted(app: &AppState, id: &str, proxy: &str) -> Option<String> {
    app.read(id, |ctx, s| {
        Ok(ctx
            .hub
            .engine
            .universe(s.character.build.ruleset)
            .get(proxy)
            .and_then(|p| match p.el.rules.first() {
                Some(Rule::Grant(g)) => Some(g.id.clone()),
                _ => None,
            }))
    })
    .ok()
    .flatten()
}

fn last_line(app: &AppState, id: &str) -> Option<String> {
    app.read(id, |_, s| {
        Ok(s.character
            .build
            .inventory
            .items
            .last()
            .map(|i| i.uid.clone()))
    })
    .ok()
    .flatten()
}

fn registered(app: &AppState, id: &str, element: &str) -> bool {
    app.read(id, |ctx, s| {
        let snap = ctx.hub.engine.derive(&s.character.build);
        Ok(snap.elements.iter().any(|r| r.element.id == element))
    })
    .unwrap_or(false)
}

/// The first per-class spell category of the edition ("Additional <Cast> Spell").
fn cast_category(app: &AppState, rs: Ruleset) -> Option<String> {
    let hub = app.hub().ok()?;
    let plain = generated("Spell");
    let cats = hub.engine.universe(rs).proxies().categories();
    cats.into_iter()
        .find(|c| c != &plain && c.ends_with(" Spell"))
}

/// With a feat proxy equipped (a hidden line), the sheet prints no "Additional ..." gear row.
fn sheet_leaves_out_hidden_lines(app: &AppState, id: &str) -> bool {
    let Some((_, proxy, name)) = search(app, id, &generated("Feat"), 1) else {
        return false;
    };
    if app.add_item(id, &proxy, 1, false, None).is_err() {
        return false;
    }
    let Some(uid) = last_line(app, id) else {
        return false;
    };
    let _ = app.equip_item(id, &uid, EquipRequest::Auto);
    let html = app
        .render_sheet(id, &SheetOptionsDto::default())
        .map(|s| s.html)
        .unwrap_or_default();
    let _ = app.remove_item(id, &uid, true);
    !html.is_empty() && !html.contains(&name)
}
