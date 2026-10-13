//! Directory load order of the custom content folder.

/// Rank groups in loading order; later groups override earlier ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LoadGroup {
    /// `srd/`
    Srd,
    /// `system-reference-document/`
    SystemReferenceDocument,
    /// `core/`
    Core,
    /// `supplements/`
    Supplements,
    /// `unearthed-arcana/`
    UnearthedArcana,
    /// `third-party/`
    ThirdParty,
    /// `homebrew/`
    Homebrew,
    /// Any other folder.
    Other,
    /// Files in sub-folders of `user/` (but not directly in `user/`).
    UserSubfolders,
    /// Files directly in the content root.
    RootFiles,
    /// Files directly in `user/`.
    UserFiles,
}

/// Classify a path relative to the content root (any mix of `/` and `\` separators).
#[must_use]
pub fn classify(relative_path: &str) -> LoadGroup {
    let normalized = relative_path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').filter(|p| !p.is_empty()).collect();
    let Some(first) = parts.first() else {
        return LoadGroup::RootFiles;
    };
    if parts.len() == 1 {
        return LoadGroup::RootFiles;
    }
    match first.to_lowercase().as_str() {
        "srd" => LoadGroup::Srd,
        "system-reference-document" => LoadGroup::SystemReferenceDocument,
        "core" => LoadGroup::Core,
        "supplements" => LoadGroup::Supplements,
        "unearthed-arcana" => LoadGroup::UnearthedArcana,
        "third-party" => LoadGroup::ThirdParty,
        "homebrew" => LoadGroup::Homebrew,
        "user" if parts.len() == 2 => LoadGroup::UserFiles,
        "user" => LoadGroup::UserSubfolders,
        _ => LoadGroup::Other,
    }
}

/// True for paths under the `custom/ignore` parking folder, which the loader skips.
#[must_use]
pub fn is_ignored_path(relative_path: &str) -> bool {
    let normalized = relative_path.replace('\\', "/").to_lowercase();
    let parts: Vec<&str> = normalized.split('/').collect();
    parts
        .windows(2)
        .any(|w| w[0] == "custom" && w[1] == "ignore")
}

/// Sort relative paths into load order: by group, then by path (within-group order
/// is unspecified by the format; a stable alphabetical order is used).
#[must_use]
pub fn sorted(paths: &[String]) -> Vec<String> {
    let mut out = paths.to_vec();
    out.sort_by(|a, b| {
        classify(a)
            .cmp(&classify(b))
            .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
            .then_with(|| a.cmp(b))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_follow_the_spec_order() {
        let paths: Vec<String> = [
            "user/mine.xml",
            "root.xml",
            "user/sub/deep.xml",
            "misc/x.xml",
            "Homebrew/h.xml",
            "third-party/t.xml",
            "unearthed-arcana/u.xml",
            "supplements/s.xml",
            "core/c.xml",
            "system-reference-document/r.xml",
            "srd/a.xml",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let order = sorted(&paths);
        assert_eq!(
            order,
            [
                "srd/a.xml",
                "system-reference-document/r.xml",
                "core/c.xml",
                "supplements/s.xml",
                "unearthed-arcana/u.xml",
                "third-party/t.xml",
                "Homebrew/h.xml",
                "misc/x.xml",
                "user/sub/deep.xml",
                "root.xml",
                "user/mine.xml",
            ]
        );
    }

    #[test]
    fn backslashes_and_ignore_folder() {
        assert_eq!(classify("core\\a.xml"), LoadGroup::Core);
        assert_eq!(classify(""), LoadGroup::RootFiles);
        assert!(is_ignored_path("custom/ignore/a.xml"));
        assert!(is_ignored_path("x\\Custom\\Ignore\\a.xml"));
        assert!(!is_ignored_path("custom/keep/ignore.xml"));
    }

    #[test]
    fn within_group_is_alphabetical_and_deterministic() {
        let paths = vec![
            "b.xml".to_string(),
            "A.xml".to_string(),
            "a.xml".to_string(),
        ];
        assert_eq!(sorted(&paths), ["A.xml", "a.xml", "b.xml"]);
    }
}
