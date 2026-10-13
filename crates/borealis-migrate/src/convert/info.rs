//! `<info>` blocks and `.index` files.

use super::util::{opt_attr, required_attr, CResult, ConvertError, Ctx};
use crate::dom::XmlElement;
use borealis_content::{
    Author, Extra, IndexEntry, IndexFile, IndexFormat, Info, IssueCode, Update, UpdateFile, Version,
};

fn check_version(el: &XmlElement, text: &str) -> CResult<()> {
    Version::parse(text.trim())
        .map(|_| ())
        .map_err(|e| ConvertError::new(IssueCode::InvalidVersion, e.to_string(), el))
}

/// `<info>`. `update` is optional here but, when present, needs a valid version
/// and a complete `<file name url/>` if a file node exists.
pub(crate) fn info(ctx: &mut Ctx, node: &XmlElement) -> CResult<Info> {
    let mut out = Info::default();
    ctx.keep_attributes(&mut out.extra, "info", node, &[]);
    for child in node.elements() {
        match child.name.as_str() {
            "name" if out.name.is_none() => out.name = Some(child.text().trim().to_string()),
            "description" if out.description.is_none() => {
                out.description = Some(child.text().trim().to_string());
            }
            "author" if out.author.is_none() => {
                let mut author = Author {
                    name: child.text().trim().to_string(),
                    url: child.attr("url").map(|u| u.trim().to_string()),
                    extra: Extra::default(),
                };
                ctx.keep_attributes(&mut author.extra, "info/author", child, &["url"]);
                out.author = Some(author);
            }
            "update" if out.update.is_none() => out.update = Some(update(ctx, child)?),
            _ => ctx.keep_child(&mut out.extra, "info", child),
        }
    }
    Ok(out)
}

fn update(ctx: &mut Ctx, node: &XmlElement) -> CResult<Update> {
    let version = required_attr(node, "version")?;
    check_version(node, &version)?;
    let mut out = Update {
        version,
        revised: opt_attr(node, "revised"),
        ..Update::default()
    };
    ctx.keep_attributes(&mut out.extra, "info/update", node, &["version", "revised"]);
    for child in node.elements() {
        if child.name == "file" && out.file.is_none() {
            let mut file = UpdateFile {
                name: required_attr(child, "name")?,
                url: required_attr(child, "url")?,
                extra: Extra::default(),
            };
            ctx.keep_attributes(&mut file.extra, "info/update/file", child, &["name", "url"]);
            out.file = Some(file);
        } else {
            ctx.keep_child(&mut out.extra, "info/update", child);
        }
    }
    Ok(out)
}

/// Convert an `<index>` document root.
pub(crate) fn index_file(ctx: &mut Ctx, root: &XmlElement) -> CResult<IndexFile> {
    let mut min_app_version = None;
    if let Some(app) = root.attr("app") {
        Version::parse_app(app.trim())
            .map_err(|e| ConvertError::new(IssueCode::InvalidVersion, e.to_string(), root))?;
        min_app_version = Some(app.trim().to_string());
    }
    let mut extra = Extra::default();
    ctx.keep_attributes(&mut extra, "index", root, &["app"]);
    let mut info_block = None;
    let mut files = Vec::new();
    for child in root.elements() {
        match child.name.as_str() {
            "info" if info_block.is_none() => info_block = Some(info(ctx, child)?),
            "files" => {
                for entry in child.elements() {
                    let mut e = IndexEntry {
                        name: required_attr(entry, "name")?,
                        url: required_attr(entry, "url")?,
                        obsolete: (entry.name == "obsolete").then_some(true),
                        ..IndexEntry::default()
                    };
                    ctx.keep_attributes(&mut e.extra, "index/files/entry", entry, &["name", "url"]);
                    files.push(e);
                }
            }
            _ => ctx.keep_child(&mut extra, "index", child),
        }
    }
    let info_block = info_block.ok_or_else(|| {
        ConvertError::new(
            IssueCode::MissingAttribute,
            "<index> has no <info> block",
            root,
        )
    })?;
    Ok(IndexFile {
        format: IndexFormat,
        min_app_version,
        info: info_block,
        files,
        extra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dom::parse_document;

    #[test]
    fn info_block() {
        let el = parse_document(
            r#"<info><name> N </name><description>d</description><author url=" https://a.invalid ">Me</author>
                <update version="1.2.3" revised="2020-04-01"><file name="a.xml" url="https://e.invalid/a.xml"/></update><extra/></info>"#,
        )
        .unwrap();
        let mut ctx = Ctx::new("a.xml");
        let i = info(&mut ctx, &el).unwrap();
        assert_eq!(i.name.as_deref(), Some("N"));
        assert_eq!(
            i.author.as_ref().unwrap().url.as_deref(),
            Some("https://a.invalid")
        );
        assert_eq!(i.remote_url(), Some("https://e.invalid/a.xml"));
        assert_eq!(
            i.update.as_ref().unwrap().revised.as_deref(),
            Some("2020-04-01")
        );
        assert_eq!(i.extra.children.len(), 1);
    }

    #[test]
    fn info_errors() {
        let mut ctx = Ctx::new("a.xml");
        for bad in [
            r#"<info><update version="1"><file name="a" url="u"/></update></info>"#,
            r#"<info><update><file name="a" url="u"/></update></info>"#,
            r#"<info><update version="1.0"><file name="a"/></update></info>"#,
        ] {
            assert!(
                info(&mut ctx, &parse_document(bad).unwrap()).is_err(),
                "{bad}"
            );
        }
        let no_update = info(
            &mut ctx,
            &parse_document("<info><name>x</name></info>").unwrap(),
        )
        .unwrap();
        assert!(no_update.update.is_none());
    }

    #[test]
    fn index_with_nested_and_obsolete_entries() {
        let el = parse_document(
            r#"<index app="1"><info><update version="1.0.0"><file name="x.index" url="https://e.invalid/x.index"/></update></info>
                 <files><file name="a.xml" url="https://e.invalid/a.xml"/><file name="sub.index" url="https://e.invalid/sub.index"/>
                 <obsolete name="old.xml" url="https://e.invalid/old.xml"/></files></index>"#,
        )
        .unwrap();
        let mut ctx = Ctx::new("x.index");
        let idx = index_file(&mut ctx, &el).unwrap();
        assert_eq!(idx.min_app_version.as_deref(), Some("1"));
        assert_eq!(idx.files.len(), 3);
        assert!(idx.files[1].is_nested_index());
        assert!(idx.files[2].is_obsolete() && !idx.files[0].is_obsolete());
        assert_eq!(idx.info.remote_url(), Some("https://e.invalid/x.index"));
        let missing = parse_document("<index><files/></index>").unwrap();
        assert!(index_file(&mut ctx, &missing).is_err());
        let bad_entry =
            parse_document(r#"<index><info/><files><file name="a"/></files></index>"#).unwrap();
        assert!(index_file(&mut ctx, &bad_entry).is_err());
        let bad_app = parse_document(r#"<index app="x"><info/></index>"#).unwrap();
        assert!(index_file(&mut ctx, &bad_app).is_err());
    }
}
