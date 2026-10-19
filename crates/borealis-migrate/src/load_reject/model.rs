//! A model of the exceptions Aurora's custom-file load throws, read off the decompiled Aurora
//! Builder (paths relative to `re-private/decompiled/`). Every check names the line that throws.
//! The check runs on the raw XML (before Borealis repairs typos), as Aurora reads it.
//!
//! .NET details the checks follow: `XmlDocument.LoadXml` drops whitespace-only text nodes, keeps
//! comments and CDATA; `Convert.ToInt32(string)` accepts surrounding whitespace and one sign;
//! `Convert.ToBoolean(string)` accepts `true`/`false` in any case, trimmed; `Setter.ValueAsBool`
//! is `false` for a blank value (`Builder.Data/ElementSetters.cs:40-47`); setter lookups ignore
//! case (`ElementSetters.cs:55-63`).

use super::{Culprit, AURORA_REQUIRED_SETTERS};
use crate::dom::{XmlElement, XmlNode};
use borealis_content::IssueCode;

const PARSER: &str = "Builder.Data/Builder.Data/ElementParser.cs";
const FILE: &str = "Builder.Data/Builder.Data.Files/ElementsFile.cs";
const RULES: &str = "Builder.Data/Builder.Data.Rules.Parsers";
const TYPES: &str = "Builder.Data/Builder.Data.ElementParsers";

/// A throw inside one element: what, and where in Aurora.
struct Fault {
    code: IssueCode,
    cause: String,
    aurora: String,
}

type Check = Result<(), Fault>;

fn fault(code: IssueCode, cause: impl Into<String>, aurora: impl Into<String>) -> Fault {
    Fault {
        code,
        cause: cause.into(),
        aurora: aurora.into(),
    }
}

/// `ElementsFile.Load` reads `<info>` (`ElementsFile.cs:76-81`) and
/// `PopulateInformationSection` throws when its `<update>` or `<update><file>` is missing. The
/// cases Borealis's converter already rejects as a whole file (bad `app`/`ignore`/`version`,
/// `<file>` without `name`/`url`) are not repeated here.
pub(super) fn file_culprit(root: &XmlElement) -> Option<Culprit> {
    let info = root.child("info")?;
    let (cause, aurora) = match info.child("update") {
        None => ("<info> has no <update>", format!("{FILE}:126-129")),
        Some(update) if update.attr("version").is_some() && update.child("file").is_none() => {
            ("<info><update> has no <file>", format!("{FILE}:138-141"))
        }
        Some(_) => return None,
    };
    Some(Culprit {
        id: None,
        ty: None,
        line: info.line,
        code: IssueCode::MissingAttribute,
        cause: cause.to_string(),
        aurora,
    })
}

/// The first exception Aurora's parsers throw for this `<element>`, if any.
pub(super) fn element_culprit(el: &XmlElement) -> Option<Culprit> {
    let id = el.attr("id").map(|s| s.trim().to_string());
    let ty = el.attr("type").map(|s| s.trim().to_string());
    let checked = check_element(el, id.as_deref(), ty.as_deref());
    checked.err().map(|f| Culprit {
        id,
        ty,
        line: el.line,
        code: f.code,
        cause: f.cause,
        aurora: f.aurora,
    })
}

fn check_element(el: &XmlElement, id: Option<&str>, ty: Option<&str>) -> Check {
    // `ParseElementHeader` (ElementParser.cs:33-36) reads the attributes with
    // `GetAttributeValue`, which throws for a missing one (XmlExtension.cs:41); called outside
    // any per-element catch at DataManager.cs:281.
    for name in ["name", "type", "source", "id"] {
        if el.attr(name).is_none() {
            return Err(fault(
                IssueCode::MissingAttribute,
                format!("<element> has no {name:?} attribute"),
                "Builder.Data/Builder.Data.Extensions/XmlExtension.cs:41",
            ));
        }
    }
    let (Some(id), Some(ty)) = (id, ty) else {
        return Ok(());
    };
    let setters = common(el)?;
    typed(el, id, ty, &setters)
}

/// `ElementParser.ParseElement` (ElementParser.cs:56-72), in its order.
fn common(el: &XmlElement) -> Result<Setters<'_>, Fault> {
    let setters = parse_setters(el)?;
    parse_rules(el)?;
    spellcasting(el)?;
    equipment(el)?;
    if let Some(compendium) = el.child("compendium") {
        attr_bool(compendium, "display", &format!("{PARSER}:404"))?;
    }
    Ok(setters)
}

// ---- setters -----------------------------------------------------------------------------

/// One `<set>`: name, trimmed inner text, other attributes.
struct Setter<'a> {
    name: &'a str,
    value: String,
    node: &'a XmlElement,
}

struct Setters<'a>(Vec<Setter<'a>>);

impl Setters<'_> {
    fn get(&self, name: &str) -> Option<&Setter<'_>> {
        self.0.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }
}

/// A child node .NET keeps and iterates: elements, CDATA, and text that is not only XML
/// whitespace. Comments are left to the caller.
fn kept(node: &XmlNode) -> bool {
    match node {
        XmlNode::Text(t) => !t.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')),
        _ => true,
    }
}

fn non_comment(el: &XmlElement) -> impl Iterator<Item = &XmlNode> {
    el.children
        .iter()
        .filter(|n| !matches!(n, XmlNode::Comment(_)) && kept(n))
}

/// `ParseSetterNodes` (ElementParser.cs:223-271).
fn parse_setters(el: &XmlElement) -> Result<Setters<'_>, Fault> {
    let mut out = Vec::new();
    let Some(block) = el.child("setters") else {
        return Ok(Setters(out));
    };
    for node in non_comment(block) {
        let named = match node {
            XmlNode::Element(e) => e.attr("name").map(|n| (n, e)),
            _ => None,
        };
        let Some((name, node)) = named else {
            return Err(fault(
                IssueCode::MissingAttribute,
                "a <setters> child has no \"name\" attribute",
                format!("{PARSER}:238-241"),
            ));
        };
        out.push(Setter {
            name,
            value: node.text().trim().to_string(),
            node,
        });
    }
    let setters = Setters(out);
    if let Some(s) = setters.get("allow duplicate") {
        strict_bool(
            &s.value,
            "setter \"allow duplicate\"",
            &format!("{PARSER}:256"),
        )?;
    }
    Ok(setters)
}

// ---- rules -------------------------------------------------------------------------------

/// `ParseRules` (ElementParser.cs:284-317): every non-comment child of `<rules>` must be a
/// rule its parsers know (`Single` throws otherwise, rethrown at :315).
fn parse_rules(el: &XmlElement) -> Check {
    let Some(rules) = el.child("rules") else {
        return Ok(());
    };
    for node in non_comment(rules) {
        let XmlNode::Element(rule) = node else {
            return Err(fault(
                IssueCode::UnknownRuleTag,
                "<rules> holds text, which is not a rule",
                format!("{PARSER}:308"),
            ));
        };
        match rule.name.as_str() {
            "inline" => {}
            "grant" => grant(rule)?,
            "select" => select(rule)?,
            "stat" => stat(rule)?,
            n if n.eq_ignore_ascii_case("statistic") => stat(rule)?,
            other => {
                return Err(fault(
                    IssueCode::UnknownRuleTag,
                    format!("<{other}> in <rules> is not a rule"),
                    format!("{PARSER}:308"),
                ))
            }
        }
    }
    Ok(())
}

/// `RuleParser.ParseSetters` (RuleParser.cs:58-61).
fn rule_sets(rule: &XmlElement) -> Check {
    if rule
        .elements()
        .any(|c| c.name == "set" && c.attr("name").is_none())
    {
        return Err(fault(
            IssueCode::MissingAttribute,
            format!(
                "a <set> in a <{}> rule has no \"name\" attribute",
                rule.name
            ),
            format!("{RULES}/RuleParser.cs:60"),
        ));
    }
    Ok(())
}

fn grant(rule: &XmlElement) -> Check {
    attr_int(rule, "level", &format!("{RULES}/GrantRuleParser.cs:32"))?;
    rule_sets(rule)
}

fn select(rule: &XmlElement) -> Check {
    let at = |line: u32| format!("{RULES}/SelectRuleParser.cs:{line}");
    rule_sets(rule)?;
    attr_int(rule, "number", &at(35))?;
    attr_int(rule, "level", &at(38))?;
    attr_bool(rule, "optional", &at(47))?;
    attr_bool(rule, "existing", &at(56))?;
    let Some(ty) = rule.attr("type") else {
        // `Attributes.Type` stays null and `Type.Equals("Spell")` throws.
        return Err(fault(
            IssueCode::MissingAttribute,
            "a <select> rule has no \"type\" attribute",
            at(73),
        ));
    };
    if ty.trim() != "List" {
        return Ok(());
    }
    // Every child node, comments included, must carry an integer `id`.
    for node in rule.children.iter().filter(|n| kept(n)) {
        let id = match node {
            XmlNode::Element(e) => e.attr("id"),
            _ => None,
        };
        match id {
            Some(v) if dotnet_int(v) => {}
            _ => {
                return Err(fault(
                    IssueCode::InvalidInteger,
                    "a <select type=\"List\"> child has no integer \"id\" (a comment or text \
                     there counts as a child)",
                    at(84) + "-86",
                ))
            }
        }
    }
    Ok(())
}

fn stat(rule: &XmlElement) -> Check {
    let at = |line: u32| format!("{RULES}/StatisticRuleParser.cs:{line}");
    attr_int(rule, "level", &at(139))?;
    attr_bool(rule, "inline", &at(145))?;
    attr_bool(rule, "merge", &at(164))?;
    rule_sets(rule)
}

// ---- other common parts ------------------------------------------------------------------

/// `ParseSpellcastingSection` (ElementParser.cs:325-367).
fn spellcasting(el: &XmlElement) -> Check {
    let Some(sc) = el.child("spellcasting") else {
        return Ok(());
    };
    for (name, line) in [
        ("prepare", 336),
        ("extend", 337),
        ("allowReplace", 338),
        ("all", 340),
    ] {
        attr_bool(sc, name, &format!("{PARSER}:{line}"))?;
    }
    for child in sc.elements() {
        attr_bool(child, "known", &format!("{PARSER}:350"))?;
    }
    Ok(())
}

/// `ParseEquipmentNode` reads `FirstChild` of `<equipment><description>` (ElementParser.cs:388).
fn equipment(el: &XmlElement) -> Check {
    let Some(description) = el.child("equipment").and_then(|e| e.child("description")) else {
        return Ok(());
    };
    if description.children.iter().any(kept) {
        return Ok(());
    }
    Err(fault(
        IssueCode::UnsupportedConstruct,
        "<equipment><description> is empty",
        format!("{PARSER}:388"),
    ))
}

// ---- type parsers ------------------------------------------------------------------------

fn typed(el: &XmlElement, id: &str, ty: &str, s: &Setters) -> Check {
    let required = AURORA_REQUIRED_SETTERS
        .iter()
        .find(|(t, _)| *t == ty || (ty == "Multiclass" && *t == "Class"));
    if let Some((_, names)) = required {
        if let Some(missing) = names.iter().find(|n| s.get(n).is_none()) {
            return Err(fault(
                IssueCode::MissingRequiredSetter,
                format!("{ty} element has no {missing:?} setter"),
                format!("{PARSER}:277-279 (ValidateElementSetters)"),
            ));
        }
    }
    match ty {
        "Level" => {
            let at = format!("{TYPES}/LevelElementParser.cs:18-19");
            int_setter(s, "level", &at)?;
            int_setter(s, "experience", &at)
        }
        "Class" => multiclass_id(el).map(|_| ()),
        "Multiclass" => multiclass(el, id),
        "Spell" => spell(s),
        "Source" => source(s),
        "Companion" => companion(s),
        "Monster" => monster(s),
        "Item" | "Weapon" | "Armor" | "Magic Item" => item(el, ty, s),
        "Race" => race(s),
        "Language" => {
            for name in ["standard", "exotic", "secret", "monster"] {
                value_bool(s, name, &format!("{TYPES}/LanguageElementParser.cs:15-39"))?;
            }
            Ok(())
        }
        "Option" => value_bool(s, "default", &format!("{TYPES}/OptionParser.cs:14")).map(|_| ()),
        "Background Feature" => value_bool(
            s,
            "primary",
            &format!("{TYPES}/BackgroundFeatureParser.cs:13"),
        )
        .map(|_| ()),
        _ => Ok(()),
    }
}

/// `ClassElementParser`: a `<multiclass>` node needs an `id` (ClassElementParser.cs:23).
fn multiclass_id(el: &XmlElement) -> Result<Option<&str>, Fault> {
    let Some(node) = el.child("multiclass") else {
        return Ok(None);
    };
    node.attr("id").map(Some).ok_or_else(|| {
        fault(
            IssueCode::MissingAttribute,
            "<multiclass> has no \"id\" attribute",
            format!("{TYPES}/ClassElementParser.cs:23"),
        )
    })
}

/// A `type="Multiclass"` element goes through `MulticlassElementParser`, which parses it as a
/// class, then returns null without a `<multiclass>` node (MulticlassElementParser.cs:78; the
/// null element throws at DataManager.cs:291) and throws when that node repeats the element's
/// id (:24-27); the node's own setters and rules are parsed too (:61, :73).
fn multiclass(el: &XmlElement, id: &str) -> Check {
    let at = |line: &str| format!("{TYPES}/MulticlassElementParser.cs:{line}");
    let Some(mc_id) = multiclass_id(el)? else {
        return Err(fault(
            IssueCode::UnsupportedConstruct,
            "a Multiclass element without a <multiclass> node is null in Aurora",
            at("78") + " and Aurora_Builder/Builder.Presentation.Services.Data/DataManager.cs:291",
        ));
    };
    if mc_id == id {
        return Err(fault(
            IssueCode::MulticlassIdSameAsClass,
            "its <multiclass> id equals the element id",
            at("24-27"),
        ));
    }
    let node = el.child("multiclass").expect("checked above");
    parse_setters(node)?;
    parse_rules(node)
}

fn spell(s: &Setters) -> Check {
    let at = |line: &str| format!("{TYPES}/SpellElementParser.cs:{line}");
    int_setter(s, "level", &at("43"))?;
    for name in [
        "hasVerbalComponent",
        "hasSomaticComponent",
        "hasMaterialComponent",
        "hasRoyaltyComponent",
    ] {
        value_bool(s, name, &at("55-59"))?;
    }
    if s.get("royaltyComponent").is_some() {
        int_setter(s, "royaltyComponent", &at("60"))?;
    }
    value_bool(s, "isConcentration", &at("61"))?;
    value_bool(s, "isRitual", &at("62")).map(|_| ())
}

/// `SourceParser` (SourceParser.cs:37-67); `beta`, `league` and `legal` are only read when the
/// setter before them leaves them relevant.
fn source(s: &Setters) -> Check {
    let at = format!("{TYPES}/SourceParser.cs:37-67");
    let b = |name: &str| value_bool(s, name, &at);
    let official = b("official")?;
    for name in ["third-party", "homebrew", "core", "supplement"] {
        b(name)?;
    }
    if !b("playtest")? {
        b("beta")?;
    }
    if !b("adventure-league")? {
        b("league")?;
    }
    if official {
        b("legal")?;
    }
    b("incomplete")?;
    b("wip").map(|_| ())
}

fn companion(s: &Setters) -> Check {
    let at = |line: &str| format!("{TYPES}/CompanionElementParser.cs:{line}");
    for name in [
        "strength",
        "dexterity",
        "constitution",
        "intelligence",
        "wisdom",
        "charisma",
    ] {
        int_setter(s, name, &at("16-21"))?;
    }
    present(s, "ac", &at("22"))?;
    present(s, "alignment", &at("44"))?;
    if s.get("proficiency").is_some() {
        int_setter(s, "proficiency", &at("119"))?;
    }
    Ok(())
}

fn monster(s: &Setters) -> Check {
    let at = format!("{TYPES}/MonsterElementParser.cs:13-43");
    for name in ["size", "type", "alignment"] {
        present(s, name, &at)?;
    }
    for name in ["str", "dex", "con", "int", "wis", "cha", "ac", "hp"] {
        int_setter(s, name, &at)?;
    }
    present(s, "hd", &at)?;
    for name in [
        "speed",
        "speed:climb",
        "speed:fly",
        "speed:swim",
        "speed:burrow",
    ] {
        if s.get(name).is_some() {
            int_setter(s, name, &at)?;
        }
    }
    Ok(())
}

/// `ItemElementParser` (Item, Weapon, Armor, Magic Item).
fn item(el: &XmlElement, ty: &str, s: &Setters) -> Check {
    let at = |line: &str| format!("{TYPES}/ItemElementParser.cs:{line}");
    if let Some(v) = s
        .get("weight")
        .and_then(|w| w.node.attr("excludeEncumbrance"))
    {
        strict_bool(v, "weight attribute \"excludeEncumbrance\"", &at("48"))?;
    }
    for name in [
        "stackable",
        "stash",
        "consumable",
        "attunement",
        "inventory-hidden",
        "valuable",
    ] {
        value_bool(s, name, &at("53-85"))?;
    }
    if el.child("setters").is_none() {
        return Err(fault(
            IssueCode::MissingRequiredSetter,
            format!("{ty} element has no <setters> block"),
            at("115"),
        ));
    }
    if ty == "Magic Item" {
        for name in ["cost", "weight"] {
            if let Some(v) = s.get(name).and_then(|x| x.node.attr("override")) {
                strict_bool(
                    v,
                    &format!("{name} attribute \"override\""),
                    &format!("{TYPES}/MagicItemElementParser.cs:18-26"),
                )?;
            }
        }
    }
    Ok(())
}

/// `RaceElementParser` reads `AdditionalAttributes["type"]` of every `names` setter
/// (RaceElementParser.cs:16-18; the name match there is case-sensitive).
fn race(s: &Setters) -> Check {
    if s.0
        .iter()
        .any(|x| x.name == "names" && x.node.attr("type").is_none())
    {
        return Err(fault(
            IssueCode::MissingRequiredSetter,
            "a \"names\" setter has no \"type\" attribute",
            format!("{TYPES}/RaceElementParser.cs:18"),
        ));
    }
    Ok(())
}

// ---- value helpers -----------------------------------------------------------------------

fn present(s: &Setters, name: &str, at: &str) -> Check {
    if s.get(name).is_none() {
        return Err(fault(
            IssueCode::MissingRequiredSetter,
            format!("no {name:?} setter (its value is read unchecked)"),
            at,
        ));
    }
    Ok(())
}

/// `Setter.ValueAsInteger` on a setter that must exist (a missing one is a null dereference).
fn int_setter(s: &Setters, name: &str, at: &str) -> Check {
    present(s, name, at)?;
    let v = &s.get(name).expect("present").value;
    if dotnet_int(v) {
        return Ok(());
    }
    Err(fault(
        IssueCode::InvalidInteger,
        format!("setter {name:?} is not an integer: {v:?}"),
        at,
    ))
}

/// `Setter.ValueAsBool` of an optional setter.
fn value_bool(s: &Setters, name: &str, at: &str) -> Result<bool, Fault> {
    match s.get(name) {
        None => Ok(false),
        Some(x) if x.value.trim().is_empty() => Ok(false),
        Some(x) => strict_bool(&x.value, &format!("setter {name:?}"), at),
    }
}

fn strict_bool(v: &str, what: &str, at: &str) -> Result<bool, Fault> {
    dotnet_bool(v).ok_or_else(|| {
        fault(
            IssueCode::InvalidBoolean,
            format!("{what} is not true/false: {v:?}"),
            at,
        )
    })
}

fn attr_int(el: &XmlElement, name: &str, at: &str) -> Check {
    match el.attr(name) {
        Some(v) if !dotnet_int(v) => Err(fault(
            IssueCode::InvalidInteger,
            format!("<{}> attribute {name:?} is not an integer: {v:?}", el.name),
            at,
        )),
        _ => Ok(()),
    }
}

fn attr_bool(el: &XmlElement, name: &str, at: &str) -> Check {
    match el.attr(name) {
        Some(v) => strict_bool(v, &format!("<{}> attribute {name:?}", el.name), at).map(|_| ()),
        None => Ok(()),
    }
}

/// `Convert.ToInt32(string)`: `NumberStyles.Integer` (surrounding whitespace, one sign), i32.
fn dotnet_int(v: &str) -> bool {
    let t = v.trim_matches(|c: char| matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' '));
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) && t.parse::<i32>().is_ok()
}

/// `Convert.ToBoolean(string)` / `bool.Parse`: `true`/`false` in any case, whitespace and NUL
/// trimmed.
fn dotnet_bool(v: &str) -> Option<bool> {
    let t = v.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    if t.eq_ignore_ascii_case("true") {
        Some(true)
    } else if t.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotnet_conversions() {
        for ok in ["1", " 12 ", "+3", "-4", "2147483647"] {
            assert!(dotnet_int(ok), "{ok}");
        }
        for bad in ["", "1.5", "x", "2147483648", "+-1", "1 2", "--1"] {
            assert!(!dotnet_int(bad), "{bad}");
        }
        assert_eq!(dotnet_bool(" TRUE "), Some(true));
        assert_eq!(dotnet_bool("False"), Some(false));
        for bad in ["", "yes", "1", "0"] {
            assert_eq!(dotnet_bool(bad), None, "{bad}");
        }
    }
}
