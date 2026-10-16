//! Stages 3 and 4: per-element generation with kind-specific prompts and SRD few-shots, then the
//! validation (schema, store load, engine sweep) with targeted repairs.

use borealis_ai::task::prompt::fence;
use borealis_ai::task::validate::{issues_to_strings, ContentValidator, JsonSchemaValidator};
use borealis_ai::task::{run_task, AiRunner, RunOptions};
use borealis_ai::AiError;
use borealis_content::file::PackOrigin;
use borealis_content::ContentStore;
use serde_json::{json, Value};

use super::fewshot::{core_catalogue, exemplars_for};
use super::normalize::{gate_element, normalize, static_checks};
use super::steps::{element_schema, ElementOut, Step};
use super::types::{
    DraftElement, Edition, ElementStatus, ImportDraft, ImportPlan, ImportProgress, ParentLink,
    PlannedElement, Role, Stage,
};
use crate::balance::load_into_engine;
use crate::homebrew::PackContext;
pub(crate) use retry::refused;
use retry::{fill_batches, generate_single, stopped, Retries};

/// One single-element generation call with the first import's retry of a transient failure.
pub(crate) async fn generate_single_once(
    runner: &dyn AiRunner,
    g: &GenCtx<'_>,
    planned: &PlannedElement,
    opts: &RunOptions<'_>,
    progress: Progress<'_>,
) -> Result<(Value, Vec<String>, u32, Vec<String>), String> {
    generate_single(runner, g, planned, opts, progress, &mut Retries::default()).await
}

#[path = "printed_terms.rs"]
mod printed_terms;
#[path = "retry.rs"]
mod retry;
#[path = "stat_blocks.rs"]
mod stat_blocks;
#[path = "validate.rs"]
mod validate;
#[cfg(test)]
pub(crate) use validate::keep_printed;
pub(crate) use validate::{push_new, validate_and_repair, validate_rounds};

/// Repair rounds per element.
pub const MAX_REPAIR_ROUNDS: u32 = 2;

/// How a generated description keeps the source (a literal, for `concat!`).
macro_rules! printed {
    () => {
        "`description` in HTML paragraphs with the source's own text kept as printed (its sentences word for word: no rewording, no summary, lead-in or sentence of your own; game terms exactly as printed, a check, saving throw or sense worded as printed and never turned into a standard phrasing)"
    };
}

/// The batch prompt's instruction for class and subclass features.
pub(crate) const FEATURE_BATCH_EACH: &str = concat!(
    "Each: ",
    printed!(),
    ", and `rules` that implement it (resources, grants, selects, stat changes); a feature with no mechanics may have no rules. Reuse existing core ids. The class's own spell slots, cantrip and spell choices and preparation are generated for you: describe them in prose only. Spells a feature grants (a spell table) and counters from class table columns are added from the outline; every other mechanic of the feature is yours to write."
);

/// The guidance for a monster: its printed stat block, as printed.
pub(crate) const MONSTER_GUIDANCE: &str = "A monster: the stat block in `data` as in the example, every number as printed (XP and proficiency bonus included: never put the standard value for the challenge rating in place of a printed one; a printed value that differs from it goes in notes), traits, actions and senses in their printed wording. When the passage prints several creatures in one block (a rider and its mount), this creature gets only the lines under its own sub-heading.";

/// The plan's summary of an element only orients the model; it never removes source content.
pub(crate) const BRIEF_NEVER_OMITS: &str = "The summary under \"What the source says\" only orients you and never removes source content: write every line of the source passage (ability score, size, speed, language and proficiency lines included) as text and as rules, even where a summary or note says to leave lines out.";

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
        (Role::Class, _) => concat!("A Class element. Include: `setters` hd and short; `rules`: saving-throw grants, one skill `select` (type Proficiency) and weapon/armor grants, each limited with requirements \"!<multiclass id>\" like the example; `data`: hit_die, primary_abilities, saving_throws, subclass_level, asi_levels, progression (EVERY array has exactly 20 entries: proficiency_bonus plus EVERY other column of the class table, keyed in snake_case, numbers as numbers and \"—\" as 0, e.g. a points pool, uses or a die size; only the spell slot columns are generated), and the multiclass definition (2024: data.multiclass {prerequisite, requirements}; 2014: top-level multiclass {id, prerequisite, requirements, rules}). Do NOT add `Class Feature` grants; they are added for you. The multiclassing section, when the source has one, gives the multiclass prerequisite and the proficiencies a multiclass character gains (write those grants in the multiclass definition). ", printed!(), " for the class's own text (not its features)."),
        (Role::ClassFeature, _) => concat!("A Class Feature: ", printed!(), " and `rules` that implement it (resources, grants, selects, stat changes). A feature with no mechanics may have no rules. Reuse existing core ids for skills, proficiencies and abilities."),
        (Role::SubclassFeature, _) => concat!("An Archetype Feature: ", printed!(), ", plus rules implementing it, as in the example."),
        (Role::Subclass, _) => concat!("An Archetype (subclass) element: ", printed!(), " (its own text only, never a digest of its features). Feature grants and `supports` are added for you."),
        (Role::Part | Role::Option, "Class Feature" | "Archetype Feature") => concat!("A named part or option of a feature: ", printed!(), ", and `rules` implementing it (resources, grants, selects, stat changes) like the example, and `prerequisite` with the COMPLETE printed prerequisite if it has one (every part: level, another feature, subclass). Its grant or select and the requirements for the prerequisite are added for you."),
        (_, "Racial Trait") => concat!("A species trait: ", printed!(), ", and `rules` implementing it (stat, grant, select) like the example. It is granted to the species for you."),
        (_, "Background Feature") => concat!("A background feature: ", printed!(), "; `rules` only for real mechanics. It is granted to the background for you."),
        (_, "Sub Race" | "Species Trait Option") => concat!("A subrace or lineage: ", printed!(), ", plus the rules it adds itself (ability scores, speed, size, languages). Only the traits the brief names as separate elements are granted for you (write no rules for those); every other line, such as a line granting a language, a speed or an ability score line, is a rule of this element itself. `supports` is set for you."),
        (Role::Text, _) | (_, "Rules Glossary Entry") => "A text-only rules or lore entry: `description` with the COMPLETE text of the section as HTML paragraphs (every sentence; table rows as a list). No rules.",
        (_, "Race" | "Species") => concat!("A species: size/speed setters, the ability score, size, speed and language rules, traits as rules (grants/selects/stat) unless the source summary says they are separate elements, and ", printed!(), "."),
        (_, "Background") => concat!("A background: skill and tool proficiencies, languages, equipment, an origin feat (2024), and ", printed!(), ". Named features listed as separate elements are granted for you."),
        (_, "Feat") => concat!("A feat: `prerequisite` with the COMPLETE printed prerequisite (every part: level, ability score, another feat or feature, proficiency, spellcasting), rules (stat/grant/select), and ", printed!(), ". Requirements for a prerequisite on another feat or feature are added for you."),
        (_, "Spell") => if edition == Edition::Y2024 { concat!("A spell with setters level, school, time, range, duration, components and `supports` listing EVERY class the source names for it, as printed (a class from another source or outside the core rules included, never dropped) (2024 rules), and the text after the stat block as ", printed!(), ".") } else { concat!("A spell with setters level, school, time, range, duration, components and `supports` listing EVERY class the source names for it, as printed (a class from another source or outside the core rules included, never dropped), and the text after the stat block as ", printed!(), ".") },
        (_, "Magic Item") => concat!("A magic item with rarity and attunement setters, a `type` setter (Wondrous item, Ring, Rod, Staff, Wand, Potion, Scroll, or Weapon/Armor with the base item as addition, like the example), rules for bonuses, and ", printed!(), "; never a placeholder or a note about the source in it (doubts go in notes)."),
        (_, "Monster") => MONSTER_GUIDANCE,
        _ => "A content element as in the example.",
    }
}

/// The part of the source relevant to `planned`.
pub(crate) fn source_window(source: &str, planned: &PlannedElement) -> String {
    super::source::element_window(source, planned)
}

/// The source of a single-element call: its window, and for a class root also the
/// multiclassing section and the unplanned prose sub-sections its window does not reach.
fn generation_source(source: &str, planned: &PlannedElement, plan: &ImportPlan) -> String {
    let mut out = source_window(source, planned);
    if !matches!(planned.role, Role::Class | Role::Subclass) {
        return out;
    }
    let extra = super::passage::multiclass_section(source, planned)
        .into_iter()
        .chain(
            super::passage::unplanned_sections(source, planned, plan)
                .into_iter()
                .map(|s| format!("## {}\n{}", s.title, s.lines.join("\n"))),
        );
    for x in extra {
        let first = x.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        if !first.is_empty() && !out.contains(first) {
            out.push_str(&format!("\n...\n{x}"));
        }
    }
    out
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
Guidance: {guide}\n\nWhat the source says: {brief}\n{never}\n\n\
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
        brief = super::fidelity::sanitize_brief(&planned.brief),
        never = BRIEF_NEVER_OMITS,
        cat = core_catalogue(g.store),
        kinds = super::rules::catalogue(),
        src = fence("user_input", &generation_source(&g.draft.source_text, planned, &g.draft.plan)),
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

pub(crate) fn siblings_of(draft: &ImportDraft, except: &str) -> Vec<Value> {
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
    run_element_call(runner, g, planned, name, previous, instruction, opts)
        .await
        .map_err(|e| e.to_string())
}

/// [`run_element`] with the model error kept, so the caller can retry it.
pub(crate) async fn run_element_call(
    runner: &dyn AiRunner,
    g: &GenCtx<'_>,
    planned: &PlannedElement,
    name: &str,
    previous: Option<(&Value, &[String])>,
    instruction: Option<&str>,
    opts: &RunOptions<'_>,
) -> Result<(Value, Vec<String>, u32), AiError> {
    let user = prompt_for(g, planned, previous, instruction);
    let step = make_step(name, g, planned, siblings_of(g.draft, &planned.id), user);
    let res = run_task(runner, &step, opts).await?;
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
            checks: Vec::new(),
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
    let mut retries = Retries::default();
    // A creature without a printed stat block is never sent for numbers.
    stat_blocks::keep_unprinted_as_text(draft);
    let alone = fill_batches(runner, ctx, &store, draft, opts, progress, &mut retries).await;
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
        let after = alone.iter().find(|(id, _)| *id == planned.id);
        if let Some((_, err)) = after.filter(|_| retries.exhausted()) {
            let e = &mut draft.elements[idx];
            e.status = ElementStatus::Failed;
            e.issues = vec![stopped(err)];
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
            generate_single(runner, &g, &planned, opts, progress, &mut retries).await
        };
        let e = &mut draft.elements[idx];
        match result {
            Ok((el, notes, attempts, retried)) => {
                push_new(&mut e.normalised, retried);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::anything::fidelity::tests::planned;

    /// Counts calls and refuses them all.
    struct NoModel(std::sync::atomic::AtomicUsize);

    impl AiRunner for NoModel {
        fn run<'a>(
            &'a self,
            _req: &'a borealis_ai::AiRequest,
            _sink: Option<&'a tokio::sync::mpsc::UnboundedSender<borealis_ai::AiEvent>>,
        ) -> borealis_ai::task::BoxFuture<'a, Result<borealis_ai::AiOutcome, AiError>> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async { Err(AiError::Invalid("no model in this test".into())) })
        }
    }

    #[tokio::test]
    async fn an_unknown_field_of_a_complete_rule_costs_no_repair_call() {
        // Arrange: a generated trinket whose rules carry a flag the engine does not read.
        let p = planned("ID_T_FEAT_GLIM", "Glim", "Feat", Role::Standalone, None);
        let mut draft: ImportDraft = serde_json::from_value(json!({"edition": "2024",
            "id_prefix": "ID_T", "source_name": "T", "pack_id": "t", "pack_name": "T",
            "source_kind": "text", "source_text": "Glim. You glow.\n",
            "detection": {"kind": "feat", "edition": "2024", "confidence": 1.0, "items": []},
            "plan": {"edition": "2024", "groups": [], "elements": [], "notes": []},
            "elements": [], "sweep_issues": []}))
        .unwrap();
        draft.plan.elements = vec![p.clone()];
        draft.elements = pending(&draft.plan);
        draft.elements[0].status = ElementStatus::Ok;
        draft.elements[0].json = json!({"id": p.id, "name": "Glim", "type": "Feat",
            "source": "T", "description": "<p>You glow.</p>", "rules": [
                {"kind": "resource", "id": "glim", "name": "Glim",
                 "max": {"type": "fixed", "value": 1}, "recharge": ["long_rest"],
                 "equipped": true}]});
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        let base = borealis_content::ContentStoreBuilder::new()
            .add_pack_dir(&root.join("core"))
            .unwrap();
        let ctx = PackContext {
            base,
            ruleset: draft.edition.ruleset(),
        };
        let runner = NoModel(0.into());
        // Act
        validate_and_repair(&runner, &ctx, &mut draft, &RunOptions::default(), None).await;
        // Assert
        let e = &draft.elements[0];
        assert_eq!(runner.0.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(e.json["rules"][0].get("equipped").is_none(), "{}", e.json);
        assert!(e.normalised.iter().any(|n| n.contains("\"equipped\"")));
        assert_ne!(e.status, ElementStatus::Failed, "{:?}", e.issues);
    }

    #[test]
    fn a_class_root_is_generated_with_its_multiclassing_section() {
        let src = "## Lamplighter\n\nLamplighters keep the roads lit.\n\n## Other Things\n\n### Multiclassing\n\nPrerequisite: Wisdom 13. You gain light armor proficiency.\n";
        let mut root = planned("c0", "Lamplighter", "Class", Role::Class, Some(1));
        root.excerpt.as_mut().unwrap().end_line = 3;
        let plan: ImportPlan = serde_json::from_value(json!({
            "edition": "2024", "groups": [], "elements": [], "notes": []}))
        .unwrap();
        let out = generation_source(src, &root, &plan);
        assert!(out.contains("Lamplighters keep the roads lit."));
        assert!(out.contains("### Multiclassing"));
        assert!(out.contains("light armor proficiency"));
    }

    #[test]
    fn prompts_keep_the_printed_wording_and_ignore_omission_briefs() {
        for g in [
            FEATURE_BATCH_EACH,
            guidance(
                &planned("t", "T", "Racial Trait", Role::Part, None),
                Edition::Y2024,
            ),
            guidance(
                &planned("t", "T", "Spell", Role::Standalone, None),
                Edition::Y2014,
            ),
        ] {
            assert!(g.contains("kept as printed"), "{g}");
            assert!(!g.contains("own words"), "{g}");
        }
        assert!(BRIEF_NEVER_OMITS.contains("never removes source content"));
    }

    #[test]
    fn a_subrace_writes_its_base_lines_and_leaves_only_the_briefed_traits() {
        let g = guidance(
            &planned("t", "T", "Sub Race", Role::Standalone, None),
            Edition::Y2014,
        );
        assert!(
            g.contains("traits the brief names as separate elements"),
            "{g}"
        );
        assert!(g.contains("a line granting a language"), "{g}");
        assert!(!g.contains("Its named traits are separate elements"), "{g}");
    }
}
