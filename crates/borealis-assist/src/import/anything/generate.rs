//! Stages 3 and 4: per-element generation with kind-specific prompts and SRD few-shots, then the
//! validation (schema, store load, engine sweep) with targeted repairs.

use std::collections::BTreeSet;

use borealis_ai::task::prompt::fence;
use borealis_ai::task::validate::{issues_to_strings, ContentValidator, JsonSchemaValidator};
use borealis_ai::task::{run_task, AiRunner, RunOptions};
use borealis_content::file::PackOrigin;
use borealis_content::{ContentStore, Element};
use serde_json::{json, Value};

use super::fewshot::{core_catalogue, exemplars_for};
use super::normalize::{gate_element, normalize, static_checks};
use super::steps::{element_schema, ElementOut, Step};
use super::sweep::{engine_sweep, SweepIssue};
use super::types::{
    DraftElement, Edition, ElementStatus, ImportDraft, ImportProgress, ParentLink, PlannedElement,
    Role, Stage,
};
use crate::balance::{balance_report, load_into_engine, Baselines};
use crate::homebrew::PackContext;

/// Repair rounds per element.
pub const MAX_REPAIR_ROUNDS: u32 = 2;

pub(crate) type Progress<'a> = Option<&'a (dyn Fn(ImportProgress) + Send + Sync)>;

pub(crate) fn emit(p: Progress<'_>, stage: Stage, done: usize, total: usize, message: &str) {
    if let Some(f) = p {
        f(ImportProgress {
            stage,
            done,
            total,
            message: message.to_string(),
        });
    }
}

pub(crate) fn guidance(planned: &PlannedElement, edition: Edition) -> &'static str {
    if let Some(g) = super::items::guidance(&planned.ty) {
        return g;
    }
    match (planned.role, planned.ty.as_str()) {
        (Role::Class, _) => "A Class element. Include: `setters` hd and short; `rules`: saving-throw grants, one skill `select` (type Proficiency) and weapon/armor grants, each limited with requirements \"!<multiclass id>\" like the example; `data`: hit_die, primary_abilities, saving_throws, subclass_level, asi_levels, progression (EVERY array has exactly 20 entries: proficiency_bonus plus EVERY other column of the class table, keyed in snake_case, numbers as numbers and \"—\" as 0, e.g. a points pool, uses or a die size; only the spell slot columns are generated), and the multiclass definition (2024: data.multiclass {prerequisite, requirements}; 2014: top-level multiclass {id, prerequisite, requirements, rules}). Do NOT add `Class Feature` grants; they are added for you.",
        (Role::ClassFeature, _) => "A Class Feature: `description` in HTML paragraphs that restates the source in your own words and `rules` that implement it (resources, grants, selects, stat changes). A feature with no mechanics may have no rules. Reuse existing core ids for skills, proficiencies and abilities.",
        (Role::SubclassFeature, _) => "An Archetype Feature: description plus rules implementing it, as in the example.",
        (Role::Subclass, _) => "An Archetype (subclass) element with a description. Feature grants and `supports` are added for you.",
        (Role::Part | Role::Option, "Class Feature" | "Archetype Feature") => "A named part or option of a feature: `description` in HTML paragraphs restating the source in your own words, and `rules` implementing it (resources, grants, selects, stat changes) like the example, and `prerequisite` with the COMPLETE printed prerequisite if it has one (every part: level, another feature, subclass). Its grant or select and the requirements for the prerequisite are added for you.",
        (_, "Racial Trait") => "A species trait: `description` in HTML paragraphs restating the source in your own words, and `rules` implementing it (stat, grant, select) like the example. It is granted to the species for you.",
        (_, "Background Feature") => "A background feature: `description` in HTML paragraphs restating the source in your own words; `rules` only for real mechanics. It is granted to the background for you.",
        (_, "Sub Race" | "Species Trait Option") => "A subrace or lineage: description, plus the rules it adds itself (ability scores, speed, size, languages). Its named traits are separate elements granted for you; `supports` is set for you.",
        (Role::Text, _) | (_, "Rules Glossary Entry") => "A text-only rules or lore entry: `description` with the COMPLETE text of the section as HTML paragraphs (every sentence; table rows as a list). No rules.",
        (_, "Race" | "Species") => "A species: size/speed setters, the ability score, size, speed and language rules, traits as rules (grants/selects/stat) unless the source summary says they are separate elements, description.",
        (_, "Background") => "A background: skill and tool proficiencies, languages, equipment, an origin feat (2024), description. Named features listed as separate elements are granted for you.",
        (_, "Feat") => "A feat: `prerequisite` with the COMPLETE printed prerequisite (every part: level, ability score, another feat or feature, proficiency, spellcasting), rules (stat/grant/select), description. Requirements for a prerequisite on another feat or feature are added for you.",
        (_, "Spell") => if edition == Edition::Y2024 { "A spell with setters level, school, time, range, duration, components and `supports` listing EVERY class the source names for it, as printed (a class from another source or outside the core rules included, never dropped) (2024 rules)." } else { "A spell with setters level, school, time, range, duration, components and `supports` listing EVERY class the source names for it, as printed (a class from another source or outside the core rules included, never dropped)." },
        (_, "Magic Item") => "A magic item with rarity and attunement setters, rules for bonuses, description. A magic weapon or armor names its base item in the `type` setter like the example.",
        (_, "Monster") => "A monster: the stat block in `data` as in the example.",
        _ => "A content element as in the example.",
    }
}

/// The part of the source relevant to `planned`.
pub(crate) fn source_window(source: &str, planned: &PlannedElement) -> String {
    super::source::element_window(source, planned)
}

fn content_schema() -> &'static JsonSchemaValidator {
    static SCHEMA: std::sync::OnceLock<JsonSchemaValidator> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(|| {
        let schema: Value = serde_json::from_str(borealis_content::schema::CONTENT_SCHEMA_JSON)
            .expect("bundled content schema is valid JSON");
        JsonSchemaValidator::from_schema(&schema).expect("bundled content schema compiles")
    })
}

fn wrap(el: &Value) -> Value {
    json!({"format": borealis_content::file::ContentFormat::ID, "elements": [el]})
}

/// Schema, store-load and structural problems of one element (sibling elements are in the store).
pub(crate) fn element_issues(
    ctx: &PackContext,
    siblings: &[Value],
    el: &Value,
    planned: &PlannedElement,
    plan: &super::types::ImportPlan,
) -> Vec<String> {
    let doc = wrap(el);
    // The content crate's own schema knows `data`, `ruleset` and every 2024 rule kind; the
    // generation schema of borealis-ai predates them.
    let schema = content_schema().validate(&doc);
    if !schema.is_empty() {
        return issues_to_strings(&schema);
    }
    let mut base = ctx.base.clone();
    if !siblings.is_empty() {
        let sib =
            json!({"format": borealis_content::file::ContentFormat::ID, "elements": siblings});
        if let Ok(f) = borealis_content::file::ContentFile::from_json_str(&sib.to_string()) {
            base = base.add_file("import-siblings", PackOrigin::Custom, f);
        }
    }
    let check = load_into_engine(&base, ctx.ruleset, &doc);
    let mut out: Vec<String> = check
        .dangling_refs
        .into_iter()
        .chain(check.store_errors)
        .collect();
    out.extend(static_checks(el, planned, plan));
    out
}

pub(crate) struct GenCtx<'a> {
    pub ctx: &'a PackContext,
    pub store: &'a ContentStore,
    pub draft: &'a ImportDraft,
    pub parent: Option<ParentLink>,
}

fn prompt_for(
    g: &GenCtx,
    planned: &PlannedElement,
    previous: Option<(&Value, &[String])>,
    instruction: Option<&str>,
) -> String {
    let caster = g.draft.plan.groups.iter().any(|x| {
        x.root_id.as_deref() == Some(&planned.id)
            && x.outline.as_ref().is_some_and(|o| o.caster().is_some())
    });
    let prefer: Vec<&str> = g.parent.iter().map(|p| p.class_name.as_str()).collect();
    let shots = exemplars_for(g.store, g.draft.edition, &planned.ty, caster, &prefer);
    let shots = serde_json::to_string_pretty(&shots).unwrap_or_default();
    let spell = g
        .draft
        .plan
        .groups
        .iter()
        .find(|x| {
            x.root_id.as_deref() == Some(&planned.id)
                || x.root_id.as_deref() == planned.owner.as_deref()
        })
        .and_then(|x| x.outline.as_ref())
        .and_then(|o| o.caster())
        .map(|s| {
            format!(
                "\nSpellcasting: ability {}, progression {}, list {:?}. Do NOT write slot, cantrip, prepared or spellcasting rules or tables: they are generated from this profile.",
                s.ability, s.progression, s.list
            )
        })
        .unwrap_or_default();
    let mut p = format!(
        "Write the element \"{name}\" (type {ty}, edition {ed:?}). Use EXACTLY id \"{id}\".{lvl}{spell}\n\
Guidance: {guide}\n\nWhat the source says: {brief}\n\n\
Real examples from the bundled SRD of the same kind (copy the SHAPE, not the content):\n{shots}\n\n\
Existing core ids you must reuse instead of inventing new ones:\n{cat}\n\n{kinds}\n\n\
Answer {{\"element\": {{...}}, \"notes\": [...]}}; put doubts in notes.{pools}\n\n{src}",
        name = planned.name,
        ty = planned.ty,
        ed = g.draft.edition,
        id = planned.id,
        lvl = planned.level.map(|l| format!(" It is gained at level {l}.")).unwrap_or_default(),
        guide = guidance(planned, g.draft.edition),
        pools = super::pools::pools_note(g.draft, std::slice::from_ref(planned)),
        brief = planned.brief,
        cat = core_catalogue(g.store),
        kinds = super::rules::catalogue(),
        src = fence("user_input", &source_window(&g.draft.source_text, planned)),
    );
    if let Some((el, issues)) = previous {
        let head = if issues.is_empty() {
            "The current element (change only what the user asks):".to_string()
        } else {
            format!(
                "The current element has these problems; fix exactly these and keep everything else:\n{}",
                issues.iter().map(|i| format!("- {i}")).collect::<Vec<_>>().join("\n")
            )
        };
        p.push_str(&format!(
            "\n\n{head}\n\n{}",
            fence("previous_output", &el.to_string())
        ));
    }
    if let Some(i) = instruction {
        p.push_str(&format!("\n\nThe user asks: {}", fence("user_input", i)));
    }
    p
}

fn make_step(
    name: &str,
    g: &GenCtx,
    planned: &PlannedElement,
    siblings: Vec<Value>,
    user: String,
) -> Step<ElementOut> {
    let ctx = g.ctx.clone();
    let plan = g.draft.plan.clone();
    let parent = g.parent.clone();
    let source_name = g.draft.source_name.clone();
    let pl = planned.clone();
    Step::new(
        name,
        "write one content element for the Borealis engine",
        "Output {\"element\": {...}, \"notes\": [...]} only. The element follows the borealis-content/1 element shape.",
        user,
        element_schema(),
    )
    .with_check(move |out: &ElementOut| {
        let mut el = out.element.clone();
        normalize(&mut el, &pl, &plan, parent.as_ref(), &source_name);
        element_issues(&ctx, &siblings, &el, &pl, &plan)
    })
}

fn siblings_of(draft: &ImportDraft, except: &str) -> Vec<Value> {
    draft
        .elements
        .iter()
        .filter(|e| e.id != except && !e.json.is_null() && e.status != ElementStatus::Failed)
        .map(|e| e.json.clone())
        .collect()
}

/// Generate (or repair, or revise) one element; returns the normalized JSON and the model's notes.
pub(crate) async fn run_element(
    runner: &dyn AiRunner,
    g: &GenCtx<'_>,
    planned: &PlannedElement,
    name: &str,
    previous: Option<(&Value, &[String])>,
    instruction: Option<&str>,
    opts: &RunOptions<'_>,
) -> Result<(Value, Vec<String>, u32), String> {
    let user = prompt_for(g, planned, previous, instruction);
    let step = make_step(name, g, planned, siblings_of(g.draft, &planned.id), user);
    let res = run_task(runner, &step, opts)
        .await
        .map_err(|e| e.to_string())?;
    let mut el = res.output.element;
    normalize(
        &mut el,
        planned,
        &g.draft.plan,
        g.parent.as_ref(),
        &g.draft.source_name,
    );
    Ok((el, res.output.notes, res.attempts))
}

/// A fresh pending draft element for each planned element.
pub(crate) fn pending(plan: &super::types::ImportPlan) -> Vec<DraftElement> {
    plan.elements
        .iter()
        .map(|p| DraftElement {
            id: p.id.clone(),
            name: p.name.clone(),
            ty: p.ty.clone(),
            role: p.role,
            level: p.level,
            json: Value::Null,
            status: ElementStatus::Pending,
            issues: Vec::new(),
            excerpt: p.excerpt.clone(),
            balance: None,
            repair_rounds: 0,
            revisions: 0,
            rule_errors: Vec::new(),
            normalised: Vec::new(),
        })
        .collect()
}

pub(crate) fn parent_of(draft: &ImportDraft, planned: &PlannedElement) -> Option<ParentLink> {
    let root = if matches!(planned.role, Role::Subclass) {
        Some(planned.id.as_str())
    } else {
        planned.owner.as_deref()
    };
    draft
        .plan
        .groups
        .iter()
        .find(|g| g.root_id.as_deref() == root)
        .and_then(|g| g.parent.clone())
}

/// Stage 3: generate every pending element (features first, roots last, so references resolve).
pub(crate) async fn fill_pending(
    runner: &dyn AiRunner,
    ctx: &PackContext,
    draft: &mut ImportDraft,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
) {
    let store = ctx.base.clone().build();
    let total = draft.elements.len();
    fill_batches(runner, ctx, &store, draft, opts, progress, total).await;
    for idx in 0..total {
        if draft.elements[idx].status != ElementStatus::Pending {
            continue;
        }
        let Some(planned) = draft
            .plan
            .elements
            .iter()
            .find(|p| p.id == draft.elements[idx].id)
            .cloned()
        else {
            continue;
        };
        emit(progress, Stage::Generate, idx, total, &planned.name);
        if planned.role == Role::Text && planned.excerpt.is_some() {
            // Rules and lore keep the source passage as it is: no model call.
            let mut el = super::parts::text_element(&planned, &draft.plan, &draft.source_text);
            normalize(&mut el, &planned, &draft.plan, None, &draft.source_name);
            let e = &mut draft.elements[idx];
            e.json = el;
            e.status = ElementStatus::Ok;
            continue;
        }
        if planned.role == Role::SubclassGate {
            let mut el = gate_element(&draft.plan, &planned);
            normalize(&mut el, &planned, &draft.plan, None, &draft.source_name);
            let e = &mut draft.elements[idx];
            e.json = el;
            e.status = ElementStatus::Ok;
            continue;
        }
        let parent = parent_of(draft, &planned);
        let result = {
            let g = GenCtx {
                ctx,
                store: &store,
                draft,
                parent,
            };
            run_element(runner, &g, &planned, "import_generate", None, None, opts).await
        };
        let e = &mut draft.elements[idx];
        match result {
            Ok((el, notes, attempts)) => {
                e.json = el;
                e.repair_rounds = 0;
                if notes.is_empty() {
                    e.status = if attempts > 1 {
                        ElementStatus::Repaired
                    } else {
                        ElementStatus::Ok
                    };
                } else {
                    e.status = ElementStatus::NeedsReview;
                    e.issues = notes;
                }
            }
            Err(msg) => {
                e.status = ElementStatus::Failed;
                e.issues = vec![format!("generation failed: {msg}")];
            }
        }
    }
}

/// Generate the pending class and subclass features in batches (several per call).
async fn fill_batches(
    runner: &dyn AiRunner,
    ctx: &PackContext,
    store: &ContentStore,
    draft: &mut ImportDraft,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
    total: usize,
) {
    for batch in super::batch::plan_batches(draft) {
        let first = draft.elements.iter().position(|e| e.id == batch[0].id);
        emit(
            progress,
            Stage::Generate,
            first.unwrap_or(0),
            total,
            &format!("{} elements", batch.len()),
        );
        let parent = super::batch::batch_parent(draft, &batch);
        let result = {
            let g = GenCtx {
                ctx,
                store,
                draft,
                parent,
            };
            super::batch::run_batch(runner, &g, &batch, opts).await
        };
        for (i, p) in batch.iter().enumerate() {
            let Some(e) = draft.elements.iter_mut().find(|e| e.id == p.id) else {
                continue;
            };
            match &result {
                Ok((els, attempts)) => {
                    let (el, notes) = els[i].clone();
                    e.json = el;
                    e.repair_rounds = 0;
                    e.status = match (notes.is_empty(), *attempts > 1) {
                        (false, _) => ElementStatus::NeedsReview,
                        (true, true) => ElementStatus::Repaired,
                        (true, false) => ElementStatus::Ok,
                    };
                    e.issues = notes;
                }
                Err(msg) => {
                    e.status = ElementStatus::Failed;
                    e.issues = vec![format!("generation failed: {msg}")];
                }
            }
        }
    }
}

/// Stage 4: engine sweep with up to [`MAX_REPAIR_ROUNDS`] targeted repairs per element, then the
/// balance flags. Unresolved sweep problems mark their element `Failed`.
pub(crate) async fn validate_and_repair(
    runner: &dyn AiRunner,
    ctx: &PackContext,
    draft: &mut ImportDraft,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
) {
    let store = ctx.base.clone().build();
    for e in &mut draft.elements {
        if e.status == ElementStatus::Failed && !e.json.is_null() {
            e.status = ElementStatus::Repaired;
            e.issues.clear();
        }
        e.repair_rounds = 0;
    }
    let mut remaining = Vec::new();
    let mut remaining_rules = Vec::new();
    for round in 0..=MAX_REPAIR_ROUNDS {
        tidy_rules(draft);
        let issues = engine_sweep(ctx, &draft.plan, &draft.elements);
        let mut rule_issues = rule_sweep(draft);
        if round == 0 {
            // Mechanics left in the text get one repair, never more.
            rule_issues.extend(super::mechanics::sweep(draft));
        }
        emit(
            progress,
            Stage::Validate,
            round as usize,
            MAX_REPAIR_ROUNDS as usize + 1,
            &format!(
                "{} sweep issue(s), {} rule issue(s)",
                issues.len(),
                rule_issues.len()
            ),
        );
        let clean = issues.is_empty() && rule_issues.is_empty();
        if clean || round == MAX_REPAIR_ROUNDS {
            remaining = issues;
            remaining_rules = rule_issues;
            break;
        }
        let all: Vec<SweepIssue> = issues.iter().chain(&rule_issues).cloned().collect();
        let blamed = blamed_elements(draft, &all);
        let mut repaired_any = false;
        for (id, msgs) in blamed {
            let Some(idx) = draft.elements.iter().position(|e| e.id == id) else {
                continue;
            };
            let e = &draft.elements[idx];
            if e.json.is_null()
                || e.repair_rounds >= MAX_REPAIR_ROUNDS
                || e.role == Role::SubclassGate
            {
                continue;
            }
            let Some(planned) = draft.plan.elements.iter().find(|p| p.id == id).cloned() else {
                continue;
            };
            let parent = parent_of(draft, &planned);
            let prev = e.json.clone();
            let result = {
                let g = GenCtx {
                    ctx,
                    store: &store,
                    draft,
                    parent,
                };
                run_element(
                    runner,
                    &g,
                    &planned,
                    "import_repair",
                    Some((&prev, &msgs)),
                    None,
                    opts,
                )
                .await
            };
            let e = &mut draft.elements[idx];
            e.repair_rounds += 1;
            repaired_any = true;
            if let Ok((el, notes, _)) = result {
                e.json = el;
                e.status = ElementStatus::Repaired;
                e.issues = notes;
            }
        }
        if !repaired_any {
            remaining = issues;
            remaining_rules = rule_issues;
            break;
        }
    }
    strip_unrepaired_rules(draft, &remaining_rules);
    mark_unresolved(draft, &remaining);
    draft.sweep_issues = remaining;
    super::mechanics::note_leftovers(draft);
    attach_balance(ctx, &store, draft);
}

/// Deterministic rule fixes on every generated element (recorded on the element).
fn tidy_rules(draft: &mut ImportDraft) {
    for e in draft.elements.iter_mut().filter(|e| !e.json.is_null()) {
        let notes = super::rules::normalise_resources(&mut e.json);
        push_new(&mut e.normalised, notes);
    }
    let known: Vec<(String, String)> = draft
        .elements
        .iter()
        .flat_map(|e| super::rules::resources_of(&e.json))
        .collect();
    for e in draft.elements.iter_mut().filter(|e| !e.json.is_null()) {
        let notes = super::rules::normalise_references(&mut e.json, &known);
        push_new(&mut e.normalised, notes);
    }
}

fn push_new(into: &mut Vec<String>, items: Vec<String>) {
    for i in items {
        if !into.contains(&i) {
            into.push(i);
        }
    }
}

/// Rule-catalogue problems of every generated element, as repairable issues.
fn rule_sweep(draft: &ImportDraft) -> Vec<SweepIssue> {
    draft
        .elements
        .iter()
        .filter(|e| !e.json.is_null() && e.status != ElementStatus::Failed)
        .flat_map(|e| {
            super::rules::rule_problems(&e.json)
                .into_iter()
                .map(|message| SweepIssue {
                    level: 0,
                    element_id: Some(e.id.clone()),
                    message,
                })
        })
        .collect()
}

/// Last resort after the repair rounds: drop what the engine would ignore, and say so on the
/// element (it needs review; the import report lists it as an error).
fn strip_unrepaired_rules(draft: &mut ImportDraft, remaining: &[SweepIssue]) {
    for e in &mut draft.elements {
        if !remaining
            .iter()
            .any(|i| i.element_id.as_deref() == Some(&e.id))
        {
            continue;
        }
        let removed = super::rules::strip_invalid(&mut e.json);
        if removed.is_empty() {
            continue;
        }
        if matches!(e.status, ElementStatus::Ok | ElementStatus::Repaired) {
            e.status = ElementStatus::NeedsReview;
        }
        push_new(&mut e.issues, removed.clone());
        push_new(&mut e.rule_errors, removed);
    }
}

fn blamed_elements(draft: &ImportDraft, issues: &[SweepIssue]) -> Vec<(String, Vec<String>)> {
    let mut order: Vec<String> = Vec::new();
    let mut map: std::collections::BTreeMap<String, BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for i in issues {
        let Some(id) = i
            .element_id
            .clone()
            .filter(|id| draft.element(id).is_some())
        else {
            continue;
        };
        let id = match draft.element(&id) {
            Some(e) if e.role == Role::SubclassGate => draft
                .plan
                .elements
                .iter()
                .find(|p| p.id == id)
                .and_then(|p| p.owner.clone())
                .unwrap_or(id),
            _ => id,
        };
        if !order.contains(&id) {
            order.push(id.clone());
        }
        let msg = if i.level > 0 {
            format!("level {}: {}", i.level, i.message)
        } else {
            i.message.clone()
        };
        map.entry(id).or_default().insert(msg);
    }
    order
        .into_iter()
        .map(|id| {
            let m = map.remove(&id).unwrap_or_default().into_iter().collect();
            (id, m)
        })
        .collect()
}

fn mark_unresolved(draft: &mut ImportDraft, remaining: &[super::sweep::SweepIssue]) {
    for i in remaining {
        let Some(id) = &i.element_id else { continue };
        if let Some(e) = draft.elements.iter_mut().find(|e| &e.id == id) {
            e.status = ElementStatus::Failed;
            let msg = if i.level > 0 {
                format!("level {}: {}", i.level, i.message)
            } else {
                i.message.clone()
            };
            if !e.issues.contains(&msg) {
                e.issues.push(msg);
            }
        }
    }
}

fn attach_balance(ctx: &PackContext, store: &ContentStore, draft: &mut ImportDraft) {
    let baselines = Baselines::from_store(store, ctx.ruleset);
    for e in &mut draft.elements {
        e.balance = None;
        if e.json.is_null()
            || matches!(
                e.role,
                Role::ClassFeature
                    | Role::SubclassFeature
                    | Role::SubclassGate
                    | Role::Part
                    | Role::Option
                    | Role::Text
            )
        {
            continue;
        }
        if let Ok(el) = serde_json::from_value::<Element>(e.json.clone()) {
            let report = balance_report(&baselines, &el);
            if report
                .flags
                .iter()
                .any(|f| f.severity != crate::balance::Severity::Ok)
            {
                e.balance = Some(report);
            }
        }
    }
}
