//! Character tie-ins (B-222, decision U17b): a character links to one note of the vault, kept in the character file
//! as a vault-relative path (`vault_note`), and a note can be created from the character.
//!
//! - A link is accepted only for a Markdown note that is in the open vault now (the same path checks as every other
//!   vault command, so nothing outside the vault and nothing hidden).
//! - A created note is `Characters/<Name>.md`; it never replaces a file: a taken name becomes `<Name> 2`, `<Name> 3`...
//!   It holds YAML front matter filled from the character (name, species, classes with levels, background, level,
//!   `tags: [character]`) and the headings Backstory, Notes and Sessions. Nothing syncs back after it is created.

use super::{file_set, store::VaultStore, tree, tree::EntryKind, MAX_NAME_TRIES};
use crate::dto::CharacterSnapshot;
use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, ContentHub};
use borealis_character::Character;

/// The folder created notes go in.
pub const CHARACTER_FOLDER: &str = "Characters";
/// The note name used when the character's name has nothing usable in a file name.
const FALLBACK_STEM: &str = "Character";
/// Longest note name taken from a character name, in characters (leaves room for the number and `.md`).
const MAX_STEM_CHARS: usize = 120;

/// What a created note says about the character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteFacts {
    pub name: String,
    pub species: Option<String>,
    /// Each class with its level, main class first.
    pub classes: Vec<(String, u32)>,
    pub background: Option<String>,
    pub level: u32,
}

/// The facts of `character` as the content defines them.
#[must_use]
pub fn facts(hub: &ContentHub, character: &Character) -> NoteFacts {
    let snap = hub.engine.derive_with(&character.build, false);
    let named = |types: &[&str]| {
        snap.elements
            .iter()
            .find(|e| types.contains(&e.element.ty.as_str()))
            .map(|e| e.element.name.trim().to_string())
            .filter(|n| !n.is_empty())
    };
    NoteFacts {
        name: character.meta.name.trim().to_string(),
        species: named(&["Race", "Species"]),
        classes: snap
            .classes
            .iter()
            .filter_map(|c| c.class.as_ref().map(|e| (e.name.clone(), c.level)))
            .collect(),
        background: named(&["Background"]),
        level: snap.level,
    }
}

/// A YAML double-quoted scalar: always valid, whatever the text holds.
fn yaml_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The text of a new character note. Empty facts are left out of the front matter.
#[must_use]
pub fn note_text(f: &NoteFacts) -> String {
    let mut lines = vec!["---".to_string(), format!("name: {}", yaml_str(&f.name))];
    if let Some(s) = &f.species {
        lines.push(format!("species: {}", yaml_str(s)));
    }
    if !f.classes.is_empty() {
        lines.push("classes:".to_string());
        lines.extend(
            f.classes
                .iter()
                .map(|(name, level)| format!("  - {}", yaml_str(&format!("{name} {level}")))),
        );
    }
    if let Some(b) = &f.background {
        lines.push(format!("background: {}", yaml_str(b)));
    }
    lines.push(format!("level: {}", f.level));
    lines.push("tags: [character]".to_string());
    lines.push("---".to_string());
    for heading in ["Backstory", "Notes", "Sessions"] {
        lines.push(String::new());
        lines.push(format!("## {heading}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

/// A note name from a character name: characters a file name or an Obsidian link cannot hold become `-`.
#[must_use]
pub fn note_stem(name: &str) -> String {
    let stem: String = name.chars().take(MAX_STEM_CHARS).collect();
    let stem = super::safe_name(&stem);
    // `safe_name` falls back to "Attachment"; a character note falls back to its own word.
    if stem == "Attachment" && name.trim() != "Attachment" {
        FALLBACK_STEM.to_string()
    } else {
        stem
    }
}

/// The `n`-th candidate path for a note named `stem`: `Characters/<stem>.md`, then `<stem> 2.md`, `<stem> 3.md`...
#[must_use]
pub fn candidate(stem: &str, n: usize) -> String {
    let name = if n == 0 {
        format!("{stem}.md")
    } else {
        format!("{stem} {}.md", n + 1)
    };
    super::paths::join(CHARACTER_FOLDER, &name)
}

/// Create the note `stem` in `store` with `text`, at the first free candidate path; returns that path. A file that
/// appears at a name meanwhile is never replaced: the next name is tried.
///
/// # Errors
/// `invalid_argument` when every name is taken or the folder is in the way; `io_error` when it cannot be written.
pub fn create_numbered(store: &dyn VaultStore, stem: &str, text: &str) -> ApiResult<String> {
    let files = file_set(&store.list());
    for n in 0..MAX_NAME_TRIES {
        let rel = candidate(stem, n);
        if files.get(&rel).is_some() || store.exists(&rel) {
            continue;
        }
        match store.create_new(&rel, text.as_bytes()) {
            Ok(()) => return Ok(rel),
            // Someone took the name between the check and the write.
            Err(_) if store.exists(&rel) => {}
            Err(e) => return Err(e),
        }
    }
    Err(ApiError::invalid("Too many notes have that name."))
}

/// Check that `path` names a Markdown note in `store`.
///
/// # Errors
/// `invalid_argument` for a path outside the vault or another kind of file; `not_found` when it is not there.
pub fn check_note(store: &dyn VaultStore, path: &str) -> ApiResult<()> {
    let real_name = store.file(path)?;
    let kinds = [super::paths::file_name(path), real_name.as_str()].map(tree::kind_of);
    if kinds.iter().all(|k| *k == EntryKind::Note) {
        Ok(())
    } else {
        Err(ApiError::invalid(
            "Only a Markdown note can be linked to a character.",
        ))
    }
}

impl AppState {
    /// Link the character to the vault note `path`, or unlink it (`None`). The note must be in the open vault.
    ///
    /// # Errors
    /// `not_found` for a closed character or a missing note; `invalid_argument` without a vault, for a path outside
    /// it or for a file that is not a note.
    pub fn set_character_note(&self, id: &str, path: Option<&str>) -> ApiResult<CharacterSnapshot> {
        if let Some(p) = path {
            check_note(self.vault()?.as_ref(), p)?;
        }
        self.mutate(id, |_, d| {
            d.character.vault_note = path.map(str::to_string);
            Ok(())
        })
    }

    /// Create a note for the character from its facts and link it.
    ///
    /// # Errors
    /// `not_found` for a closed character; `invalid_argument` without a vault; `io_error` when it cannot be written.
    pub fn vault_create_character_note(&self, id: &str) -> ApiResult<CharacterSnapshot> {
        self.mutate(id, |ctx, d| {
            let store = ctx.app.vault()?;
            let facts = facts(ctx.hub, &d.character);
            let rel = create_numbered(store.as_ref(), &note_stem(&facts.name), &note_text(&facts))?;
            d.character.vault_note = Some(rel);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aria() -> NoteFacts {
        NoteFacts {
            name: "Aria".into(),
            species: Some("Elf".into()),
            classes: vec![("Wizard".into(), 3), ("Fighter".into(), 1)],
            background: Some("Sage".into()),
            level: 4,
        }
    }

    #[test]
    fn the_note_holds_front_matter_and_the_three_headings() {
        let text = note_text(&aria());
        assert_eq!(
            text,
            "---\nname: \"Aria\"\nspecies: \"Elf\"\nclasses:\n  - \"Wizard 3\"\n  - \"Fighter 1\"\nbackground: \"Sage\"\n\
             level: 4\ntags: [character]\n---\n\n## Backstory\n\n## Notes\n\n## Sessions\n"
        );
        // The vault's own front matter reader sees the tag.
        let fm = crate::vault::syntax::front_matter(&text).expect("front matter");
        assert_eq!(
            crate::vault::syntax::front_matter_tags(&text[fm.yaml]),
            vec!["character"]
        );
    }

    fn yaml_name(text: &str) -> String {
        use yaml_rust2::YamlLoader;
        let fm = crate::vault::syntax::front_matter(text).expect("front matter");
        let docs = YamlLoader::load_from_str(&text[fm.yaml]).expect("valid YAML");
        docs[0]["name"].as_str().expect("name").to_string()
    }

    #[test]
    fn empty_facts_are_left_out_and_odd_text_stays_valid_yaml() {
        let f = NoteFacts {
            name: "Ka\"zu: \\the #1\nbold".into(),
            species: None,
            classes: Vec::new(),
            background: None,
            level: 1,
        };
        let text = note_text(&f);
        assert!(
            !text.contains("species") && !text.contains("classes") && !text.contains("background")
        );
        assert_eq!(yaml_name(&text), f.name);
    }

    #[test]
    fn names_are_made_safe_and_numbered_from_two() {
        assert_eq!(note_stem("Aria"), "Aria");
        assert_eq!(note_stem("Ser [Bold]: #1?"), "Ser -Bold-- -1-");
        assert_eq!(note_stem("  ..  "), "Character");
        assert_eq!(
            note_stem("x".repeat(500).as_str()).chars().count(),
            MAX_STEM_CHARS
        );
        assert_eq!(candidate("Aria", 0), "Characters/Aria.md");
        assert_eq!(candidate("Aria", 1), "Characters/Aria 2.md");
        assert_eq!(candidate("Aria", 2), "Characters/Aria 3.md");
    }
}
