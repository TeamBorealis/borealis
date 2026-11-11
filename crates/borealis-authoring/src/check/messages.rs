//! Plain-language messages (spec 8.2): every content validator code ([`IssueCode`]), every rule problem
//! ([`RuleProblem`]), every sweep finding ([`SweepProblem`]), the engine's refusals, the balance hints and the
//! creator's own findings ([`Finding`]) as one sentence a user can act on. No ids, no field keys, no jargon: an id
//! is shown by the name of its element (the last name the form knew, else the loaded element's), or not at all.
//!
//! Every `match` here is exhaustive, so a new code can't be added without its sentence; `messages_tests.rs` checks
//! every variant (read from the enum's source) and that each sentence is plain.

use super::balance::{BalanceFlag, Severity as BalanceSeverity};
use super::sweep::{SweepFinding, SweepProblem};
use crate::form::Lookup;
use borealis_content::issue::{Issue, IssueCode};
use borealis_content::rule_kinds::RuleProblem;

/// What a user can do about a problem Borealis made itself (ids are generated; spec 8.2 "internal error report").
const INTERNAL: &str =
    "Borealis couldn't build this element correctly. Please report this problem.";

/// "a spell", "an item", "domains": the noun with its article.
#[must_use]
pub fn with_article(noun: &str) -> String {
    let plural = noun.ends_with('s') && !noun.ends_with("ss") && !noun.ends_with("us");
    if plural {
        return noun.to_string();
    }
    let vowel = noun
        .chars()
        .next()
        .is_some_and(|c| "aeiouAEIOU".contains(c));
    format!("{} {noun}", if vowel { "an" } else { "a" })
}

/// The plain noun of an element type ("Archetype" is a subclass, "Monster" a creature).
#[must_use]
pub fn type_noun(ty: &str) -> String {
    match ty {
        "Archetype" => "subclass".into(),
        "Archetype Feature" => "subclass feature".into(),
        "Monster" => "creature".into(),
        "Race" => "race".into(),
        "Sub Race" => "subrace".into(),
        "Option" => "class option".into(),
        "" => "element".into(),
        other => other.to_lowercase(),
    }
}

/// Upper-cases the first letter.
fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// `snake_case` key as words ("allow_replace" -> "allow replace").
#[must_use]
pub fn words(key: &str) -> String {
    key.replace(['_', '-'], " ")
}

/// The plain noun of a required setter ([`borealis_content::validate::required_setters`]).
fn setter_noun(name: &str) -> String {
    let noun = match name.to_ascii_lowercase().as_str() {
        "level" => "a level",
        "school" => "a school",
        "time" => "a casting time",
        "duration" => "a duration",
        "range" => "a range",
        "hd" => "a hit die",
        "experience" => "an experience total",
        "alignment" => "an alignment",
        "domains" => "domains",
        "symbol" => "a symbol",
        "url" => "a web address",
        "abbreviation" => "an abbreviation",
        "size" => "a size",
        "type" => "a creature type",
        "str" | "strength" => "a Strength score",
        "dex" | "dexterity" => "a Dexterity score",
        "con" | "constitution" => "a Constitution score",
        "int" | "intelligence" => "an Intelligence score",
        "wis" | "wisdom" => "a Wisdom score",
        "cha" | "charisma" => "a Charisma score",
        "ac" => "an Armor Class",
        "hp" => "hit points",
        "challenge" => "a challenge rating",
        other => return with_article(&words(other)),
    };
    noun.to_string()
}

/// The text between the first pair of double quotes.
fn quoted(text: &str) -> Option<&str> {
    let start = text.find('"')? + 1;
    let len = text[start..].find('"')?;
    Some(&text[start..start + len])
}

/// The id a dangling-reference message ends with (`... references unknown element ID_X`).
fn last_word(text: &str) -> &str {
    text.rsplit(' ').next().unwrap_or_default()
}

/// The element's name, quoted, or `None` when nothing knows it.
fn name_of(id: &str, names: &dyn Lookup) -> Option<String> {
    names
        .find(id)
        .map(|r| r.name)
        .filter(|n| !n.trim().is_empty())
}

/// An engine or validator reason in plain words: ids become their names ("an element that isn't loaded" when no name
/// is known), code quotes are dropped, the sentence ends with a full stop.
#[must_use]
pub fn plain_reason(reason: &str, names: &dyn Lookup) -> String {
    let mut out = Vec::new();
    for word in reason.split(' ') {
        let core = word.trim_matches(|c: char| !(c.is_alphanumeric() || c == '_'));
        if core.starts_with("ID_") && core.len() > 3 {
            let shown = name_of(core, names).map_or_else(
                || "an element that isn't loaded".to_string(),
                |n| format!("\"{n}\""),
            );
            out.push(word.replace(core, &shown));
        } else if core.contains('_') {
            out.push(word.replace('_', " "));
        } else {
            out.push(word.to_string());
        }
    }
    let text = out.join(" ").replace(['`', '{', '}', '[', ']'], "");
    let text = text.trim().trim_end_matches(['.', ';', ':']).to_string();
    if text.is_empty() {
        "the engine gave no reason".into()
    } else {
        text
    }
}

/// The sentence of a content validator issue on `element_type` (`""` when unknown). `names` knows the last names of
/// referenced elements.
#[must_use]
pub fn issue_message(issue: &Issue, element_type: &str, names: &dyn Lookup) -> String {
    let noun = type_noun(element_type);
    let m = issue.message.as_str();
    match issue.code {
        IssueCode::XmlParse => "The original file isn't readable XML, so this part couldn't be brought in.".into(),
        IssueCode::MissingAttribute => {
            "The original file left out a value this part needs, so it was brought in without it.".into()
        }
        IssueCode::InvalidInteger => {
            "The original file has a number here that isn't a whole number; it was left out.".into()
        }
        IssueCode::InvalidBoolean => {
            "The original file has a yes/no value here that isn't yes or no; it was left out.".into()
        }
        IssueCode::InvalidVersion => {
            "The original file's version number can't be read; it was left out.".into()
        }
        IssueCode::UnknownRuleTag => {
            "Kept from the original: Borealis doesn't know this kind of rule.".into()
        }
        IssueCode::UnsupportedConstruct => {
            "Kept from the original: a part Borealis doesn't use (it had no effect there either).".into()
        }
        IssueCode::ElementDropped => {
            "Part of the original couldn't be brought in and was left out.".into()
        }
        IssueCode::DuplicateChild => {
            "The original repeats a part that appears only once; the extra copy is kept as is.".into()
        }
        IssueCode::Normalized => "An old spelling from the original was updated.".into(),
        IssueCode::IndexEntryMissing => {
            "A file the pack's index lists isn't on this device.".into()
        }
        IssueCode::FileSkipped => "A file of the pack was skipped (it is marked to be ignored).".into(),
        IssueCode::DuplicateId | IssueCode::InvalidId | IssueCode::MulticlassIdSameAsClass => {
            INTERNAL.into()
        }
        IssueCode::ElementReplaced => {
            "This element takes the place of an element of another pack.".into()
        }
        IssueCode::NonStandardId => {
            "This element was made outside the creator and is stored in an unusual way; it still works.".into()
        }
        IssueCode::MissingRequiredSetter => missing_setter(m, &noun),
        IssueCode::DanglingReference => dangling(issue, names),
        IssueCode::InvalidExpression => invalid_expression(m),
        IssueCode::AppendTargetMissing => {
            "The element this adds to isn't loaded any more (its pack was removed or turned off). Pick it again."
                .into()
        }
        IssueCode::DuplicateSelectName => {
            "Two choices on this element have the same label. Give one a different label.".into()
        }
        IssueCode::UndefinedSource => {
            format!("This {noun} names a book that no loaded pack defines.")
        }
        IssueCode::SchemaViolation => {
            "Part of this element can't be stored as it is. Undo the last change, or remove the part kept from the original."
                .into()
        }
        IssueCode::InvalidMonsterData => {
            "Part of this creature's stat block doesn't add up. Check the marked part.".into()
        }
        IssueCode::NonStandardMonsterXp => {
            "The experience points differ from the usual amount for this challenge rating (kept as written)."
                .into()
        }
    }
}

fn missing_setter(message: &str, noun: &str) -> String {
    if message.contains("`names`") {
        return "Each list of names needs to say what kind of names it holds.".into();
    }
    match quoted(message) {
        Some(setter) => format!(
            "{} needs {}.",
            capital(&with_article(noun)),
            setter_noun(setter)
        ),
        None => format!(
            "{} is missing a value it needs.",
            capital(&with_article(noun))
        ),
    }
}

fn dangling(issue: &Issue, names: &dyn Lookup) -> String {
    let m = issue.message.as_str();
    if m.contains("absence test always passes") {
        return "This condition checks that the character lacks something no loaded pack has, so that part always passes."
            .into();
    }
    if m.contains("alternative is never met") {
        return "One alternative in this condition names something that isn't loaded, so that alternative never applies."
            .into();
    }
    match name_of(last_word(m), names) {
        Some(name) => format!(
            "\"{name}\" isn't loaded any more (its pack was removed or turned off). Pick it again."
        ),
        None => "Something this element uses isn't loaded any more (its pack was removed or turned off). Pick it again."
            .into(),
    }
}

fn invalid_expression(message: &str) -> String {
    if message.starts_with("supports") {
        "This filter, kept from the original, can't be read. Rebuild it with the filter builder."
            .into()
    } else if message.starts_with("equipped") {
        "This \"while wearing or holding\" test, kept from the original, can't be read. Rebuild it with its builder."
            .into()
    } else {
        "This condition, kept from the original, can't be read. Rebuild it with the condition builder.".into()
    }
}

/// The sentence of a rule problem (a custom rule the engine would ignore, or ignore part of).
#[must_use]
pub fn rule_problem_message(p: &RuleProblem) -> String {
    match p {
        RuleProblem::UnknownKind(_) => {
            "Kept from the original: Borealis doesn't use this rule.".into()
        }
        RuleProblem::UnknownField { field, .. } => format!(
            "Kept from the original: Borealis doesn't use the \"{}\" setting of this rule.",
            words(field)
        ),
    }
}

/// The code of a sweep finding (`sweep.empty_select`); a store issue keeps its validator code.
#[must_use]
pub fn sweep_code(p: &SweepProblem) -> String {
    let code = match p {
        SweepProblem::Store(i) => return issue_code(i.code),
        SweepProblem::NoClassSlot => "no_class_slot",
        SweepProblem::ClassRefused(_) => "class_refused",
        SweepProblem::LevelUpFails(_) => "level_up",
        SweepProblem::SubclassRefused(_) => "subclass_refused",
        SweepProblem::SubclassNeverOffered => "subclass_never_offered",
        SweepProblem::Engine(_) => "engine",
        SweepProblem::BrokenSelection(_) => "broken_selection",
        SweepProblem::EmptySelect { .. } => "empty_select",
        SweepProblem::MissingFeature(_) => "missing_feature",
        SweepProblem::NoSpellcasting => "no_spellcasting",
        SweepProblem::SlotsShrink(_) => "slots_shrink",
        SweepProblem::SlotsFlat => "slots_flat",
        SweepProblem::NoSlots => "no_slots",
    };
    format!("sweep.{code}")
}

/// The sentence of a level sweep finding (spec 8.2: "At level 7 the choice "Arcane Shot" has nothing to offer.").
#[must_use]
pub fn sweep_message(f: &SweepFinding, element_type: &str, names: &dyn Lookup) -> String {
    let level = f.level;
    match &f.problem {
        SweepProblem::Store(i) => issue_message(i, element_type, names),
        SweepProblem::NoClassSlot => {
            "The level check couldn't run: the test character has no class to pick. Turn on the edition's core rules."
                .into()
        }
        SweepProblem::ClassRefused(e) => format!(
            "A test character couldn't take this class: {}.",
            plain_reason(e, names)
        ),
        SweepProblem::LevelUpFails(e) => format!(
            "A test character couldn't reach level {level}: {}.",
            plain_reason(e, names)
        ),
        SweepProblem::SubclassRefused(e) => format!(
            "A test character couldn't take this subclass: {}.",
            plain_reason(e, names)
        ),
        SweepProblem::SubclassNeverOffered => {
            "The class never offers this subclass. Check that its tags match the class's subclass choice.".into()
        }
        SweepProblem::Engine(m) => format!("At level {level}: {}.", plain_reason(m, names)),
        SweepProblem::BrokenSelection(_) => {
            format!("At level {level} one of the test character's choices stopped working.")
        }
        SweepProblem::EmptySelect { name, .. } => {
            format!("At level {level} the choice \"{name}\" has nothing to offer.")
        }
        SweepProblem::MissingFeature(id) => match name_of(id, names) {
            Some(n) => format!(
                "At level {level} the feature \"{n}\" doesn't appear. Check the level it is given at."
            ),
            None => format!(
                "At level {level} one of the class's features doesn't appear. Check the level it is given at."
            ),
        },
        SweepProblem::NoSpellcasting => format!(
            "At level {level} the class still can't cast spells. Check its spellcasting section."
        ),
        SweepProblem::SlotsShrink(l) => {
            format!("At level {l} the class has fewer spell slots than at the level before.")
        }
        SweepProblem::SlotsFlat => "The class's spell slots never grow from level 1 to 20.".into(),
        SweepProblem::NoSlots => "The class never gets spell slots.".into(),
    }
}

/// The sentence of an engine refusal (spec 8.2: "A test character couldn't take this species: <engine reason>.").
#[must_use]
pub fn refusal_message(element_type: &str, reason: &str, names: &dyn Lookup) -> String {
    format!(
        "A test character couldn't take this {}: {}.",
        type_noun(element_type),
        plain_reason(reason, names)
    )
}

/// The wire code of a validator code (`dangling_reference`).
#[must_use]
pub fn issue_code(code: IssueCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// What a balance metric compares, and how it reads when the value is above every SRD value / below them.
/// (`report.rs` emits these metrics; a test checks the list is complete.)
pub const BALANCE_METRICS: [(&str, &str, &str, &str); 12] = [
    (
        "spell.damage_avg",
        "SRD spells of its level",
        "deals {more} damage than any",
        "deals far less damage than any",
    ),
    (
        "feat.asi_total",
        "SRD feats",
        "grants {more} ability score increases than any",
        "grants far fewer ability score increases than any",
    ),
    (
        "feat.proficiencies",
        "SRD feats",
        "grants {more} proficiencies than any",
        "grants far fewer proficiencies than any",
    ),
    (
        "item.bonus",
        "SRD magic items of its rarity",
        "gives a {more}bigger bonus than any",
        "gives a far smaller bonus than any",
    ),
    (
        "species.trait_count",
        "SRD species",
        "has {more} traits than any",
        "has far fewer traits than any",
    ),
    (
        "species.speed",
        "SRD species",
        "is {more}faster than any",
        "is far slower than any",
    ),
    (
        "species.darkvision",
        "SRD species",
        "sees {more}farther in the dark than any",
        "sees far less in the dark than any",
    ),
    (
        "species.asi_total",
        "SRD species",
        "grants {more} ability score increases than any",
        "grants far fewer ability score increases than any",
    ),
    (
        "monster.hp",
        "SRD creatures of its challenge rating",
        "has {more} hit points than any",
        "has far fewer hit points than any",
    ),
    (
        "monster.ac",
        "SRD creatures of its challenge rating",
        "has a {more}higher Armor Class than any",
        "has a far lower Armor Class than any",
    ),
    (
        "monster.attack_bonus",
        "SRD creatures of its challenge rating",
        "has a {more}higher attack bonus than any",
        "has a far lower attack bonus than any",
    ),
    (
        "monster.dpr",
        "SRD creatures of its challenge rating",
        "deals {more} damage per round than any",
        "deals far less damage per round than any",
    ),
];

/// The balance hint of a flagged metric ("Compared with SRD feats: grants more ability score increases than any."),
/// `None` for a value within the SRD range or without a baseline.
#[must_use]
pub fn balance_message(flag: &BalanceFlag) -> Option<String> {
    if flag.severity < BalanceSeverity::Medium {
        return None;
    }
    let above = flag.value > flag.baseline_range[1];
    let (group, more, less) = BALANCE_METRICS
        .iter()
        .find(|(m, ..)| *m == flag.metric)
        .map_or(
            (
                "the SRD",
                "is {more}stronger than anything",
                "is far weaker than anything",
            ),
            |(_, g, more, less)| (*g, *more, *less),
        );
    let far = if flag.severity == BalanceSeverity::High {
        "far "
    } else {
        ""
    };
    let phrase = if above {
        more.replace("{more} ", &format!("{far}more "))
            .replace("{more}", far)
    } else {
        less.to_string()
    };
    Some(format!("Compared with {group}: {phrase}."))
}

/// The creator's own findings: form, block and lint problems, each with its code, severity and sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// The empty name of an element.
    NoName,
    /// A required field of the form is empty (`subject` "a deity", `field` "an alignment").
    RequiredField {
        /// The element as a noun phrase.
        subject: String,
        /// The field label.
        field: String,
    },
    /// A required field of a block is empty.
    BlockRequired {
        /// The block's label ("Gives a spell").
        block: String,
        /// The field label.
        field: String,
    },
    /// A number of a block outside its range.
    BlockRange {
        /// The block's label.
        block: String,
        /// The field label.
        field: String,
        /// Lowest value.
        min: i32,
        /// Highest value.
        max: i32,
    },
    /// Dice text that isn't dice.
    BlockDice {
        /// The block's label.
        block: String,
        /// The field label.
        field: String,
    },
    /// A value outside the choices the engine understands.
    BlockChoice {
        /// The block's label.
        block: String,
        /// The field label.
        field: String,
    },
    /// A value of the wrong shape (a control wrote what it shouldn't).
    BlockShape,
    /// Another element of the same kind in the pack has this name.
    DuplicateName {
        /// The kind's noun ("spell").
        noun: String,
        /// The name.
        name: String,
    },
    /// The description is empty.
    EmptyDescription,
    /// The description has escape leftovers from pasting (spec 7.3).
    Escape,
    /// A choice whose filter matches nothing loaded.
    SelectOffersNothing,
    /// A spell list no spell is on.
    SpellListEmpty,
    /// A resource cost naming a pool nothing defines.
    UnknownPool {
        /// The pool's name.
        pool: String,
    },
    /// The generic form names a type it can't write (spec 4.12): one with a form of its own, or one the pack manages.
    TypeNotHere {
        /// The type as written.
        ty: String,
        /// Where elements of the type are made ("New element > Spell"); `None` for a managed type (Source).
        made_with: Option<String>,
    },
    /// A subrace or race variant no loaded race's choice offers.
    NotOffered {
        /// "subrace", "variant".
        noun: String,
    },
}

impl Finding {
    /// The code (`block.required`).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoName | Self::RequiredField { .. } => "form.required",
            Self::BlockRequired { .. } => "block.required",
            Self::BlockRange { .. } => "block.range",
            Self::BlockDice { .. } => "block.dice",
            Self::BlockChoice { .. } => "block.choice",
            Self::BlockShape => "block.shape",
            Self::DuplicateName { .. } => "lint.duplicate_name",
            Self::EmptyDescription => "lint.empty_description",
            Self::Escape => "lint.escape",
            Self::SelectOffersNothing => "lint.select_empty",
            Self::SpellListEmpty => "lint.spell_list_empty",
            Self::UnknownPool { .. } => "lint.unknown_pool",
            Self::NotOffered { .. } => "lint.not_offered",
            Self::TypeNotHere { .. } => "form.type",
        }
    }

    /// The severity: what keeps a pack from loading is an error; the lints warn; an empty description is a hint.
    #[must_use]
    pub fn severity(&self) -> super::ProblemSeverity {
        use super::ProblemSeverity::{Error, Info, Warning};
        match self {
            Self::NoName
            | Self::RequiredField { .. }
            | Self::BlockRequired { .. }
            | Self::BlockRange { .. }
            | Self::BlockChoice { .. }
            | Self::BlockShape
            | Self::TypeNotHere { .. } => Error,
            Self::BlockDice { .. }
            | Self::DuplicateName { .. }
            | Self::Escape
            | Self::SelectOffersNothing
            | Self::SpellListEmpty
            | Self::UnknownPool { .. }
            | Self::NotOffered { .. } => Warning,
            Self::EmptyDescription => Info,
        }
    }

    /// The sentence.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NoName => "Give it a name.".into(),
            Self::RequiredField { subject, field } => {
                format!("{} needs {}.", capital(subject), with_article(&field.to_lowercase()))
            }
            Self::BlockRequired { block, field } => {
                format!("{block}: fill in {}.", field.to_lowercase())
            }
            Self::BlockRange {
                block,
                field,
                min,
                max,
            } => format!(
                "{block}: {} must be a whole number from {min} to {max}.",
                field.to_lowercase()
            ),
            Self::BlockDice { block, field } => format!(
                "{block}: write {} as dice, like 2d6 or 1d8+2.",
                field.to_lowercase()
            ),
            Self::BlockChoice { block, field } => format!(
                "{block}: pick {} from the list.",
                field.to_lowercase()
            ),
            Self::BlockShape => {
                "This value can't be read. Undo the last change or enter it again.".into()
            }
            Self::DuplicateName { noun, name } => {
                format!("Another {noun} in this pack is already called \"{name}\".")
            }
            Self::EmptyDescription => "There is no description yet.".into(),
            Self::Escape => {
                "The description has odd characters left over from copying and pasting. Use Fix to clean them up."
                    .into()
            }
            Self::SelectOffersNothing => {
                "Nothing loaded matches what this choice offers. Check what it lets the player choose from.".into()
            }
            Self::SpellListEmpty => {
                "No loaded spell is on this spell list yet. Add spells to it, or pick another list.".into()
            }
            Self::UnknownPool { pool } => format!(
                "Nothing defines the points \"{pool}\" this costs. Add the pool, or pick one that exists."
            ),
            Self::NotOffered { noun } => format!(
                "No loaded race's {noun} choice offers it. Check the parent race: it is the tag that choice offers, usually the race's name."
            ),
            Self::TypeNotHere {
                ty,
                made_with: Some(route),
            } => format!("{ty} elements have their own form: make it with {route}."),
            Self::TypeNotHere {
                made_with: None, ..
            } => {
                "The pack writes this kind of element itself (its source, a class's multiclassing): choose another type."
                    .into()
            }
        }
    }
}

#[cfg(test)]
#[path = "messages_tests.rs"]
mod tests;
