//! The homebrew pack on disk (spec 14.1): `pack.json` (the manifest), `content.json` (one content file: the pack's
//! `Source` element first, then the elements in creation order, then the appends), `pack-meta.json` (`kind:
//! homebrew`, written by the app) and `creator.json` ([`super::creator`]).

use crate::ids::{is_pack_code, IdGenerator};
use borealis_content::rich::{normalize_plain_text, render, Block, Doc, Inline};
use borealis_content::{
    ContentFile, Element, PackFile, PackFileKind, PackFormat, PackManifest, PackOrigin, Ruleset,
    Setter,
};
use std::collections::HashSet;

/// The pack's one content file.
pub const CONTENT_FILE: &str = "content.json";
/// The type of the pack's book element.
pub const SOURCE_TYPE: &str = "Source";
/// Longest pack name.
pub const PACK_NAME_MAX: usize = 80;
/// The version of a new pack.
pub const FIRST_VERSION: &str = "1.0.0";

/// What NEW PACK asks for (spec 14.1), also the pack settings later (the code never changes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSettings {
    /// Shown name; also the name of the pack's `Source` element, which is the `source` of every element.
    pub name: String,
    /// 2-8 of `A-Z0-9`, unique among loaded books' abbreviations.
    pub code: String,
    /// `dnd2014`, `dnd2024`, or `any` (Both editions, IM17).
    pub edition: Ruleset,
    /// Author, if given.
    pub author: Option<String>,
    /// Description, if given (plain text).
    pub description: Option<String>,
    /// Licence text (`All rights reserved`, `CC-BY-4.0`, `CC0`, or the user's own), if given.
    pub license: Option<String>,
}

/// Why settings cannot make a pack. Each message is one plain sentence for the NEW PACK flyout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsProblem(pub String);

impl std::fmt::Display for SettingsProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SettingsProblem {}

fn clean(text: Option<&str>) -> Option<String> {
    text.map(normalize_plain_text).filter(|t| !t.is_empty())
}

impl PackSettings {
    /// The settings with their texts normalized (NFC, trimmed, single spaces, no control characters), checked.
    ///
    /// # Errors
    /// A [`SettingsProblem`] for an empty or long name, a bad code, or a code another loaded book uses.
    pub fn checked(&self, taken_codes: &HashSet<String>) -> Result<Self, SettingsProblem> {
        let name = normalize_plain_text(&self.name);
        if name.is_empty() {
            return Err(SettingsProblem("Give the pack a name.".into()));
        }
        if name.chars().count() > PACK_NAME_MAX {
            return Err(SettingsProblem(format!(
                "The pack name can be at most {PACK_NAME_MAX} characters."
            )));
        }
        if !is_pack_code(&self.code) {
            return Err(SettingsProblem(
                "The pack code is 2 to 8 capital letters or digits.".into(),
            ));
        }
        if taken_codes.contains(&self.code) {
            return Err(SettingsProblem(format!(
                "Another book already uses the code {}.",
                self.code
            )));
        }
        Ok(Self {
            name,
            code: self.code.clone(),
            edition: self.edition,
            author: clean(self.author.as_deref()),
            description: clean(self.description.as_deref()),
            license: clean(self.license.as_deref()),
        })
    }

    /// The pack's id: `homebrew-<code>` in lower case (also its folder in the library).
    #[must_use]
    pub fn pack_id(&self) -> String {
        pack_id_for(&self.code)
    }
}

/// The pack id of a creator pack with this code.
#[must_use]
pub fn pack_id_for(code: &str) -> String {
    format!("homebrew-{}", code.to_ascii_lowercase())
}

/// The rulesets a manifest lists for a pack edition: Both editions lists both.
#[must_use]
pub fn manifest_rulesets(edition: Ruleset) -> Vec<Ruleset> {
    match edition {
        Ruleset::Any => vec![Ruleset::Dnd2014, Ruleset::Dnd2024],
        r => vec![r],
    }
}

/// The edition of a pack: its content file's default ruleset, else its manifest's only ruleset, else Both editions.
#[must_use]
pub fn pack_edition(manifest: &PackManifest, content: &ContentFile) -> Ruleset {
    content
        .ruleset
        .unwrap_or(match manifest.rulesets.as_slice() {
            [one] => *one,
            _ => Ruleset::Any,
        })
}

/// The manifest of a new creator pack (`origin: custom`, one content file).
#[must_use]
pub fn new_manifest(settings: &PackSettings) -> PackManifest {
    PackManifest {
        format: PackFormat,
        id: settings.pack_id(),
        name: settings.name.clone(),
        version: FIRST_VERSION.to_string(),
        origin: PackOrigin::Custom,
        rulesets: manifest_rulesets(settings.edition),
        license: settings.license.clone(),
        attribution: settings.author.clone(),
        description: settings.description.clone(),
        files: vec![PackFile {
            path: CONTENT_FILE.to_string(),
            kind: PackFileKind::Content,
        }],
    }
}

/// The manifest with new settings (name, edition, licence, author, description; id, version and files kept).
#[must_use]
pub fn manifest_with(manifest: &PackManifest, settings: &PackSettings) -> PackManifest {
    PackManifest {
        name: settings.name.clone(),
        rulesets: manifest_rulesets(settings.edition),
        license: settings.license.clone(),
        attribution: settings.author.clone(),
        description: settings.description.clone(),
        ..manifest.clone()
    }
}

/// The description HTML of a plain text: one paragraph, written by the one description writer (no escapes can
/// appear, spec 7).
#[must_use]
pub fn paragraph_html(text: &str) -> String {
    render(&Doc::new(vec![Block::paragraph(vec![Inline::text(text)])]))
}

/// The setters of the pack's book element: `abbreviation` = the code, `url` (empty: the validator requires the
/// setter on every book), `author`, `release` (`YYYYMMDD`).
fn source_setters(settings: &PackSettings, url: &str, release: &str) -> Vec<Setter> {
    let mut setters = vec![
        Setter::new("abbreviation", settings.code.as_str()),
        Setter::new("url", url),
    ];
    if let Some(a) = &settings.author {
        setters.push(Setter::new("author", a.as_str()));
    }
    if !release.is_empty() {
        setters.push(Setter::new("release", release));
    }
    setters
}

/// The pack's `Source` element (spec 14.1): written from the settings; `release` is the creation date `YYYYMMDD`.
#[must_use]
pub fn source_element(settings: &PackSettings, ids: &mut IdGenerator, release: &str) -> Element {
    let mut e = Element::new(
        ids.element_id(SOURCE_TYPE, &settings.name),
        settings.name.as_str(),
        SOURCE_TYPE,
        settings.name.as_str(),
    );
    e.setters = source_setters(settings, "", release);
    e.description = settings
        .description
        .as_deref()
        .map(paragraph_html)
        .unwrap_or_default();
    e
}

/// A new pack's content file: its `Source` element and the edition as the file's default ruleset.
#[must_use]
pub fn new_content(settings: &PackSettings, source: Element) -> ContentFile {
    let mut file = ContentFile::new(vec![source]);
    file.ruleset = Some(settings.edition);
    file
}

/// The pack's book element, if it has one.
#[must_use]
pub fn source_of(content: &ContentFile) -> Option<&Element> {
    content.elements.iter().find(|e| e.ty == SOURCE_TYPE)
}

/// The name new elements of the pack carry as their `source`: the pack's book element's name; for a pack without
/// one (an AI-made pack), the source most of its elements carry, else the pack name.
#[must_use]
pub fn source_name(manifest: &PackManifest, content: &ContentFile) -> String {
    if let Some(s) = source_of(content) {
        return s.name.clone();
    }
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for e in &content.elements {
        match counts.iter_mut().find(|(s, _)| *s == e.source) {
            Some(c) => c.1 += 1,
            None => counts.push((e.source.as_str(), 1)),
        }
    }
    counts
        .into_iter()
        .filter(|(s, _)| !s.is_empty())
        .max_by_key(|(_, n)| *n)
        .map_or_else(|| manifest.name.clone(), |(s, _)| s.to_string())
}

/// The content with new settings: the book element renamed and its setters rewritten (other setters kept), every
/// element that named the old book naming the new one, the edition as the file's default ruleset.
#[must_use]
pub fn content_with(content: &ContentFile, settings: &PackSettings) -> ContentFile {
    let mut next = content.clone();
    next.ruleset = Some(settings.edition);
    let Some(old) = source_of(content).map(|s| s.name.clone()) else {
        return next;
    };
    for e in &mut next.elements {
        if e.source == old {
            e.source.clone_from(&settings.name);
        }
        if e.ty != SOURCE_TYPE {
            continue;
        }
        e.name.clone_from(&settings.name);
        let release = e.setter_value("release").unwrap_or_default().to_string();
        let url = e.setter_value("url").unwrap_or_default().to_string();
        let own = ["abbreviation", "url", "author", "release"];
        let kept: Vec<Setter> = e
            .setters
            .iter()
            .filter(|s| !own.contains(&s.name.as_str()))
            .cloned()
            .collect();
        e.setters = source_setters(settings, &url, &release);
        e.setters.extend(kept);
        e.description = settings
            .description
            .as_deref()
            .map(paragraph_html)
            .unwrap_or_default();
    }
    next
}

/// The next version: the patch number bumped (`1.0.0` -> `1.0.1`), so lists and sync previews show the change.
/// A version that is not `a.b.c` numbers becomes `<version>.1`.
#[must_use]
pub fn bump_patch(version: &str) -> String {
    let parts: Vec<&str> = version.trim().split('.').collect();
    match parts.as_slice() {
        [a, b, c] if [a, b, c].iter().all(|p| p.parse::<u64>().is_ok()) => {
            let patch = c.parse::<u64>().unwrap_or(0).saturating_add(1);
            format!("{a}.{b}.{patch}")
        }
        _ if version.trim().is_empty() => FIRST_VERSION.to_string(),
        _ => format!("{}.1", version.trim()),
    }
}

/// The content with `elements` saved: each replaces the element with its id in place, new ones go after every
/// element (creation order), before nothing else (appends live apart).
#[must_use]
pub fn with_elements(content: &ContentFile, elements: &[Element]) -> ContentFile {
    let mut next = content.clone();
    for e in elements {
        match next.elements.iter_mut().find(|x| x.id == e.id) {
            Some(slot) => *slot = e.clone(),
            None => next.elements.push(e.clone()),
        }
    }
    next
}

/// The content without the elements `ids`, and the removed elements (in file order).
#[must_use]
pub fn without_elements(content: &ContentFile, ids: &HashSet<&str>) -> (ContentFile, Vec<Element>) {
    let mut next = content.clone();
    let (removed, kept): (Vec<Element>, Vec<Element>) = next
        .elements
        .into_iter()
        .partition(|e| ids.contains(e.id.as_str()));
    next.elements = kept;
    (next, removed)
}

/// A stable digest of elements (64-bit FNV-1a over their canonical JSON): a draft keeps the digest of the saved
/// elements it started from, so a later change to them (another device's pack arrived) shows (spec 10.1).
#[must_use]
pub fn digest(elements: &[&Element]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for e in elements {
        let text = serde_json::to_string(e).unwrap_or_default();
        for b in text.bytes().chain(std::iter::once(0)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}
