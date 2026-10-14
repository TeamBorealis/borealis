//! Model steps of the pipeline: a generic JSON task, the attachment-carrying runner and the
//! prompts. Every system prompt starts with `TASK: <name>` so fakes can route on it.

use borealis_ai::task::prompt::{base_system, fence};
use borealis_ai::task::{AiRunner, AiTask, BoxFuture};
use borealis_ai::{AiError, AiEvent, AiOutcome, AiRequest, AiSession, Attachment};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use super::types::{DetectedItem, Edition, ImportKind, Outline};

/// A runner that can also send attachments. Runners without vision ignore them (the default
/// method), which is the text-layer fallback.
pub trait VisionRunner: AiRunner {
    /// Run `req` with `attachments` the model can see.
    fn run_with<'a>(
        &'a self,
        req: &'a AiRequest,
        attachments: &'a [Attachment],
        sink: Option<&'a UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        let _ = attachments;
        self.run(req, sink)
    }
}

impl VisionRunner for AiSession {
    fn run_with<'a>(
        &'a self,
        req: &'a AiRequest,
        attachments: &'a [Attachment],
        sink: Option<&'a UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        Box::pin(AiSession::run_with(self, req, attachments, sink))
    }
}

/// Adapts a [`VisionRunner`] plus fixed attachments to a plain [`AiRunner`].
pub(crate) struct Attaching<'a> {
    pub inner: &'a dyn VisionRunner,
    pub attachments: &'a [Attachment],
}

impl AiRunner for Attaching<'_> {
    fn run<'a>(
        &'a self,
        req: &'a AiRequest,
        sink: Option<&'a UnboundedSender<AiEvent>>,
    ) -> BoxFuture<'a, Result<AiOutcome, AiError>> {
        self.inner.run_with(req, self.attachments, sink)
    }
}

type Check<T> = Box<dyn Fn(&T) -> Vec<String> + Send + Sync>;

/// One model step with a typed JSON result.
pub(crate) struct Step<T> {
    pub system: String,
    pub user: String,
    pub schema: Value,
    pub check: Check<T>,
}

impl<T> Step<T> {
    pub fn new(name: &str, goal: &str, extra_rules: &str, user: String, schema: Value) -> Self {
        Self {
            system: format!("TASK: {name}\n{}\n\n{extra_rules}", base_system(goal)),
            user,
            schema,
            check: Box::new(|_| Vec::new()),
        }
    }

    pub fn with_check(mut self, f: impl Fn(&T) -> Vec<String> + Send + Sync + 'static) -> Self {
        self.check = Box::new(f);
        self
    }
}

impl<T: DeserializeOwned + Serialize> AiTask for Step<T> {
    type Output = T;
    const NAME: &'static str = "import_anything";

    fn system_prompt(&self) -> String {
        self.system.clone()
    }
    fn user_prompt(&self) -> String {
        self.user.clone()
    }
    fn output_schema(&self) -> Value {
        self.schema.clone()
    }
    fn check(&self, output: &T) -> Vec<String> {
        (self.check)(output)
    }
}

// ---- outputs -------------------------------------------------------------------------------

/// Detect output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DetectOut {
    pub kind: ImportKind,
    pub edition: Edition,
    pub confidence: f64,
    pub items: Vec<DetectedItem>,
    #[serde(default)]
    pub notes: Vec<String>,
    /// Source objects that are none of the kinds.
    #[serde(default)]
    pub other: Vec<super::types::OtherItem>,
    /// Full text of the source as read (required for attachments).
    #[serde(default)]
    pub transcript: String,
}

/// Outline output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlineOut {
    pub outline: Outline,
}

/// Element generation / repair / revision output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ElementOut {
    pub element: Value,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Revision target output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TargetOut {
    pub element_id: String,
}

// ---- schemas -------------------------------------------------------------------------------

const KINDS: [&str; 9] = [
    "class",
    "subclass",
    "species",
    "background",
    "feat",
    "spell",
    "item",
    "monster",
    "mixed",
];

pub(crate) fn detect_schema() -> Value {
    json!({"type":"object","required":["kind","edition","confidence","items"],
        "properties":{
          "kind":{"type":"string","enum":KINDS},
          "edition":{"type":"string","enum":["2014","2024"]},
          "confidence":{"type":"number"},
          "items":{"type":"array","items":{"type":"object","required":["kind","name"],
            "properties":{"kind":{"type":"string","enum":KINDS},"name":{"type":"string"},
              "parent_hint":{"type":["string","null"]}}}},
          "notes":{"type":"array","items":{"type":"string"}},
          "other":{"type":"array","items":{"type":"object","required":["name"],
            "properties":{"name":{"type":"string"},"what":{"type":"string"},"note":{"type":"string"}}}},
          "transcript":{"type":"string"}}})
}

pub(crate) fn outline_schema() -> Value {
    json!({"type":"object","required":["outline"],"properties":{"outline":{"type":"object",
        "required":["name","features"],
        "properties":{"name":{"type":"string"},"summary":{"type":"string"},
          "hit_die":{"type":["integer","null"]},"subclass_level":{"type":["integer","null"]},
          "spellcasting":{"type":["object","null"]},"quote":{"type":"string"},
          "features":{"type":"array","items":{"type":"object","required":["level","name"],
            "properties":{"level":{"type":"integer"},"name":{"type":"string"},
              "summary":{"type":"string"},"quote":{"type":"string"}}}}}}}})
}

pub(crate) fn element_schema() -> Value {
    json!({"type":"object","required":["element"],"properties":{
        "element":{"type":"object","required":["id","name","type"]},
        "notes":{"type":"array","items":{"type":"string"}}}})
}

pub(crate) fn target_schema() -> Value {
    json!({"type":"object","required":["element_id"],"properties":{"element_id":{"type":"string"}}})
}

// ---- prompts -------------------------------------------------------------------------------

pub(crate) fn detect_step(
    source: &str,
    has_attachments: bool,
    edition_hint: Edition,
) -> Step<DetectOut> {
    let transcript = if has_attachments {
        "The source is attached as images or PDFs. Put the complete text you can read, in reading order, \
in `transcript`; later steps only see that transcript."
    } else {
        "Leave `transcript` empty."
    };
    let user = format!(
        "Classify this source. List every distinct thing it describes in `items` (kind, name, and for a \
subclass the parent class in `parent_hint`, as named or implied, e.g. \"Barbarian\" or \"Primal Path\"). \
Set `kind` to the single kind, or \"mixed\" when there are several. List in `other` (name as printed, \
`what` it is) every other thing the source defines that is none of these kinds: rules, weapon properties, \
conditions, variant or optional rules, upgrades, tables. Decide the edition: 2024 rules use \
\"species\", \"Weapon Mastery\", \"Origin feat\"; 2014 rules use \"race\" and \"Primal Path\"-style subclass \
names. Default to {} when unclear. Give a confidence between 0 and 1.\n{transcript}\n\n{}",
        match edition_hint {
            Edition::Y2014 => "2014",
            Edition::Y2024 => "2024",
        },
        fence("user_input", source)
    );
    Step::new(
        "import_detect",
        "classify imported D&D content by kind and edition",
        "Kinds: class, subclass, species, background, feat, spell, item, monster, mixed.",
        user,
        detect_schema(),
    )
}

fn outline_what(item: &DetectedItem, parent_levels: Option<&[u32]>) -> String {
    if item.kind == ImportKind::Subclass {
        let lv = parent_levels
            .map(|l| format!(" The parent class's subclasses gain features at levels {l:?}; use only those levels."))
            .unwrap_or_default();
        format!(
            "the subclass \"{}\" (only its own features).{lv}",
            item.name
        )
    } else {
        format!(
            "the class \"{}\": a feature for each level that grants one (include the ability score improvement / \
epic boon rows; SKIP levels that only change table numbers, they need no feature), the subclass choice level in `subclass_level`, the hit die, and spellcasting \
(a profile {{kind: full|half|third|pact|none, ability, prepared|known, ritual, spell_list_name, \
cantrips_by_level and spells_known_by_level as 20 cumulative numbers, prepared_formula}}); never write slot tables.",
            item.name
        )
    }
}

const QUOTE_RULE: &str = "For each feature give a short verbatim `quote` from the source \
(under 80 characters) so the passage can be located.";

pub(crate) fn outline_step(
    item: &DetectedItem,
    edition: Edition,
    source: &str,
    parent_levels: Option<&[u32]>,
) -> Step<OutlineOut> {
    let user = format!(
        "Outline {} Edition: {edition:?}. {QUOTE_RULE}\n\n{}",
        outline_what(item, parent_levels),
        fence("user_input", source)
    );
    Step::new(
        "import_outline",
        "outline an imported class or subclass into features by level",
        "Output {\"outline\": {...}} only.",
        user,
        outline_schema(),
    )
}

/// Combined outlines output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutlinesOut {
    pub outlines: Vec<Outline>,
}

fn same_name(a: &str, b: &str) -> bool {
    let n = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    n(a) == n(b)
}

impl OutlinesOut {
    /// The outline answering `name`, else the one at `index`.
    pub(crate) fn take(&self, name: &str, index: usize) -> Option<Outline> {
        self.outlines
            .iter()
            .find(|o| same_name(&o.name, name))
            .or_else(|| self.outlines.get(index))
            .cloned()
    }
}

/// ONE call outlining several classes and subclasses (`items` with their parent levels).
pub(crate) fn outlines_step(
    items: &[(DetectedItem, Option<Vec<u32>>)],
    edition: Edition,
    source: &str,
) -> Step<OutlinesOut> {
    let list: String = items
        .iter()
        .enumerate()
        .map(|(i, (item, lv))| format!("{}. {}\n", i + 1, outline_what(item, lv.as_deref())))
        .collect();
    let user = format!(
        "Outline each of these {n} in ONE answer, in this order, each with `name` exactly as given:\n{list}\
Edition: {edition:?}. {QUOTE_RULE}\n\n{}",
        fence("user_input", source),
        n = items.len(),
    );
    let names: Vec<String> = items.iter().map(|(i, _)| i.name.clone()).collect();
    let item_schema = outline_schema()["properties"]["outline"].clone();
    Step::new(
        "import_outline",
        "outline several imported classes or subclasses into features by level",
        "Output {\"outlines\": [{...}, ...]} only, one per requested name.",
        user,
        json!({"type":"object","required":["outlines"],
            "properties":{"outlines":{"type":"array","items":item_schema}}}),
    )
    .with_check(move |out: &OutlinesOut| {
        names
            .iter()
            .filter(|n| !out.outlines.iter().any(|o| same_name(&o.name, n)))
            .map(|n| format!("no outline named \"{n}\""))
            .collect()
    })
}
