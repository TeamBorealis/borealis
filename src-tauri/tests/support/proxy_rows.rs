//! Rows for the classic builder's generated proxy items (`specs/proxy-items.md` P8, IM13): the
//! equipment screen's "Additional ..." categories of an edition, adding and equipping one proxy
//! of each kind, and the sheet leaving its hidden line out. A proxy is never an offering host
//! (IM13 D5), so these rows are separate from the stage-2 offer rows. Ids and labels only.

use super::app::{Driver, TEST_SCORES};
use crate::{outcome, SUITE};
use borealis_content::{Rule, Ruleset};
use borealis_lib::dto_ext::{EquipRequest, ItemFilter, ItemPatch, SheetOptionsDto};
use borealis_lib::state::AppState;

/// The generated categories every edition lists (their targets include the core catalogue).
const KINDS: [(&str, &str); 5] = [
    ("feat", "Feat"),
    ("language", "Language"),
    ("proficiency", "Proficiency"),
    ("asi", "Ability Score Improvement"),
    ("spell", "Spell"),
];

/// Rows of a category a proxy is picked from.
const PICK_PAGE: usize = 200;

/// Why a row fails: a label, never content text.
type Row = Result<(), &'static str>;

fn generated(ty: &str) -> String {
    format!("Additional {ty}")
}

/// Record the proxy rows of one edition on a fresh character (its own driver, so the stage-2
/// host counts stay as they were). Every row names the edition: a source records both.
pub fn record(src: &str, app: &AppState, rs: Ruleset) {
    let d = Driver::new(app);
    let Some(host) = d.create(rs, TEST_SCORES) else {
        return;
    };
    let edition = format!("{rs:?}").to_lowercase();
    categories_row(src, app, &host.id, &edition);
    for (kind, ty) in KINDS {
        let row = add_row(app, &host.id, &generated(ty));
        record_row(
            src,
            &format!("ui/equipment/proxy-add/{kind}/{edition}"),
            row,
        );
    }
    let row = cast_category(app, rs)
        .ok_or("no spellcasting name")
        .and_then(|c| add_row(app, &host.id, &c));
    record_row(
        src,
        &format!("ui/equipment/proxy-add/cast-spell/{edition}"),
        row,
    );
    let row = sheet_leaves_out_hidden_lines(app, &host.id);
    record_row(src, &format!("sheet/hidden-line/{edition}"), row);
    d.drop_host(host);
}

fn record_row(src: &str, name: &str, row: Row) {
    outcome::record(
        SUITE,
        &format!("{src}/{name}"),
        row.is_ok(),
        row.err().unwrap_or(""),
    );
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

/// `(total, [(row id, row name)])` of a category search, at most `limit` rows.
fn search(
    app: &AppState,
    id: &str,
    category: &str,
    limit: usize,
) -> Option<(usize, Vec<(String, String)>)> {
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
    let rows = page
        .items
        .iter()
        .map(|i| (i.summary.id.clone(), i.summary.name.clone()))
        .collect();
    Some((page.total, rows))
}

/// The first proxy of `category` whose target has no requirements of its own: `(proxy id,
/// proxy name, target id)`. A granted element whose requirements fail is ungranted
/// (`specs/engine.md` 3.2 Pass A step 3), so a feat with an ability prerequisite the test scores miss
/// is rightly not registered.
fn pick(
    app: &AppState,
    id: &str,
    category: &str,
) -> Result<(String, String, String), &'static str> {
    let (_, rows) = search(app, id, category, PICK_PAGE).ok_or("category lists no proxy")?;
    if rows.is_empty() {
        return Err("category lists no proxy");
    }
    rows.into_iter()
        .find_map(|(proxy, name)| match granted(app, id, &proxy) {
            Some((target, true)) => Some((proxy, name, target)),
            _ => None,
        })
        .ok_or("no proxy whose target has no requirements")
}

/// Add the first proxy of `category`, equip it: its target is registered; unequip and remove it.
fn add_row(app: &AppState, id: &str, category: &str) -> Row {
    let (proxy, _, target) = pick(app, id, category)?;
    let uid = add_line(app, id, &proxy)?;
    let equipped = app.equip_item(id, &uid, EquipRequest::Auto).is_ok();
    let has = registered(app, id, &target);
    let _ = app.remove_item(id, &uid, true);
    match (equipped, has) {
        (false, _) => Err("equip refused"),
        (true, false) => Err("target not registered when equipped"),
        (true, true) => Ok(()),
    }
}

/// Add one line of `proxy`: its uid.
fn add_line(app: &AppState, id: &str, proxy: &str) -> Result<String, &'static str> {
    app.add_item(id, proxy, 1, false, None)
        .map_err(|_| "add refused")?;
    last_line(app, id).ok_or("no inventory line after add")
}

/// What the proxy grants (its one grant rule), and whether that target has no requirements.
fn granted(app: &AppState, id: &str, proxy: &str) -> Option<(String, bool)> {
    app.read(id, |ctx, s| {
        let uni = ctx.hub.engine.universe(s.character.build.ruleset);
        Ok(uni.get(proxy).and_then(|p| match p.el.rules.first() {
            Some(Rule::Grant(g)) => {
                let open = uni
                    .get(&g.id)
                    .is_some_and(|t| t.el.requirements.trim().is_empty());
                Some((g.id.clone(), open))
            }
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

/// With a feat proxy equipped (a hidden line), the sheet's gear list has no row of it. Its
/// feat's card still names the proxy in the left footer (the granting element,
/// `character-sheet.md` 10.3), as in Aurora, so only the gear rows are searched.
fn sheet_leaves_out_hidden_lines(app: &AppState, id: &str) -> Row {
    let (proxy, name, _) = pick(app, id, &generated("Feat"))?;
    let uid = add_line(app, id, &proxy)?;
    let equipped = app.equip_item(id, &uid, EquipRequest::Auto).is_ok();
    let hidden = render(app, id);
    // control: the same line listed in the inventory has its gear row
    let listed = ItemPatch {
        list_in_inventory: Some(true),
        ..ItemPatch::default()
    };
    let shown_when_listed = app.update_item_details(id, &uid, &listed).is_ok();
    let listed = render(app, id);
    let _ = app.remove_item(id, &uid, true);
    let shown = escaped(&name);
    let has_row = |html: &str| gear_row_names(html).any(|n| n == shown);
    if !equipped {
        Err("equip refused")
    } else if hidden.is_empty() {
        Err("sheet not rendered")
    } else if has_row(&hidden) {
        Err("the gear list has a row of the hidden line")
    } else if !shown_when_listed || !has_row(&listed) {
        Err("control: the line listed in the inventory has no gear row")
    } else {
        Ok(())
    }
}

fn render(app: &AppState, id: &str) -> String {
    app.render_sheet(id, &SheetOptionsDto::default())
        .map(|s| s.html)
        .unwrap_or_default()
}

/// The names of the sheet's gear rows (`equip.<n>.name` fields), as rendered (HTML-escaped).
fn gear_row_names(html: &str) -> impl Iterator<Item = &str> {
    html.split("data-field=\"equip.")
        .skip(1)
        .filter_map(|rest| {
            let (field, value) = rest.split_once("\">")?;
            let (_, part) = field.split_once('"')?.0.split_once('.')?;
            (part == "name").then(|| value.split("</span>").next().unwrap_or(""))
        })
}

/// `s` escaped as the sheet renders a field value.
fn escaped(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
