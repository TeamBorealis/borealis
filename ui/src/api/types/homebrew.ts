import type { BookRuleset, ElementDetail, Ruleset, SpellFacts } from "./core";
import type { CharacterSnapshot } from "./character";
import type { ContentPack } from "./content";

/**
 * Homebrew creator (1.5.0, decisions HC1-HC4; specs/homebrew-creator-spec.md, the source of truth). Hidden behind the
 * `creatorEnabled` setting until 1.5.0 ships (HC3). The user never sees an id, an expression or JSON: ids travel in these
 * types and are never shown (spec 5.1).
 *
 * Shapes marked "owned by WB-nn" are envelopes here; the work item named fills in their inner structure.
 */

/** A creator form: `spell`, `feat`, `class`, `classFeature`, `subclass`, `language`, `other`... (spec 3.1). The list
 * and each form's layout come from `homebrewKinds`. */
export type HomebrewKindId = string;

/** Where a kind sits in the New element dialog and the CATEGORIES list (spec 3.1, 13.11). */
export type HomebrewKindGroup = "main" | "small" | "child" | "expert";

/** One editor sub-tab (spec 13.3): `general`, `levels`, `spellcasting`, `rules`, `description`, `tryIt`, `data`. */
export interface HomebrewTab {
  id: string;
  /** Shown uppercase in the sub-tab band ("GENERAL"). */
  label: string;
}

/** The control a field is edited with (spec 4, 13.4). Builders and pickers are documented in spec 5-7. The second group
 * is used by block dialogs (`HomebrewBlockField`, WB-05):
 * - `auto`: written by the creator, never shown (a grant's element type, a written list's row numbers).
 * - `number` with `choices`: a number or one of those words (`any`, `all`); with `allowCustom`, any text too.
 * - `numbers`: a list of whole numbers (spell levels). `chips` with `choices`: a list from those values.
 * - `name`: a name in `scope` (a pool, a toggle, a named choice, a class...); `multiple` for a list.
 * - `statValue`: text `"2"`, a statistic, or `"-<statistic>"`. `numberOrStatistic`: a number or `{type: "stat", stat}`.
 * - `formulas`: a list of formula terms in `dialect`. `dice`: dice text (`1d10`).
 * - `uses`: a number, `"proficiency"`, an ability name, or `{type: "table", key}`.
 * - `abilitySource`: an ability name or `{param: <named choice>}`. `abilityChoice`: a number (any N abilities) or a list
 *   of abilities. `splits`: point splits (`[[2, 1], [1, 1, 1]]`). `classCounter`: a column of the class table.
 * - `group` (an object of `fields`), `rows` (a list of such objects), `variant` (an object whose `type` picks one of
 *   `variants`), `map` (an object from keys to values; `fields` = [key, value]), `blocks` (nested blocks of `kinds`). */
export type HomebrewControl =
  | "text" | "multiline" | "number" | "combo" | "check" | "toggle" | "tags" | "chips"
  | "reference" | "references" | "statistic" | "condition" | "tagFilter" | "equipped" | "formula"
  | "richText" | "sheet" | "grid" | "blocks" | "children" | "setters" | "mapsTo"
  | "auto" | "numbers" | "name" | "statValue" | "numberOrStatistic" | "formulas" | "dice" | "uses"
  | "abilitySource" | "abilityChoice" | "splits" | "classCounter" | "group" | "rows" | "variant" | "map";

/** One choice of a combo field. `value` is what the form stores; `label` is shown. */
export interface HomebrewChoice {
  value: string;
  label: string;
}

/** What a reference picker offers (spec 5.1). */
export interface HomebrewPickerTarget {
  /** Element types offered (`Spell`, `Feat`, `Proficiency`...); empty = any type. */
  types: string[];
  /** Only elements carrying one of these tags (`supports`); empty = no tag filter. */
  tags?: string[];
  /** Several picks (check boxes, Magic grid) instead of one. */
  multiple?: boolean;
}

/** One form field, bound to every model location that holds it (spec 3.3). */
export interface HomebrewFieldSpec {
  /** Form key; the per-kind layout binds this key (spec 15.3). */
  key: string;
  /** FieldHeader label, in sentence case (shown uppercase by the header). */
  label: string;
  /** The sub-tab it is on. */
  tab: string;
  /** SectionSecondary heading it is grouped under, if any. */
  section?: string;
  control: HomebrewControl;
  required: boolean;
  /** Shown only for these editions; absent = both. */
  editions?: Ruleset[];
  /** Combo choices. */
  choices?: HomebrewChoice[];
  /** A combo that also takes custom text. */
  allowCustom?: boolean;
  /** Number range. */
  min?: number;
  max?: number;
  /** Reference fields. */
  picker?: HomebrewPickerTarget;
  /** Expert: the model locations the field writes (`setter:level`, `data.level`), shown in the generated-data view. */
  bindings: string[];
  /** Plain-words help under the field. */
  help?: string;
  /** Only an expert sees it (Expert toggle, spec 6.4). */
  expert?: boolean;
  /** Shown, not edited (the Source label). */
  readOnly?: boolean;
}

/** One creator form (spec 3.1, 4). */
export interface HomebrewKindDescriptor {
  id: HomebrewKindId;
  /** "Spell". */
  label: string;
  /** "Spells" (CATEGORIES list). */
  pluralLabel: string;
  /** One line for the New element card (spec 13.11). */
  description: string;
  group: HomebrewKindGroup;
  /** Element types this form writes (`Spell`; `Class` for a class, whose features are the `classFeature` kind). */
  elementTypes: string[];
  /** For a child kind: the kinds it is edited inside (`classFeature` inside `class`). */
  parentKinds: HomebrewKindId[];
  /** Editions the kind exists in (`race` is 2014, `species` 2024). */
  editions: Ruleset[];
  tabs: HomebrewTab[];
  fields: HomebrewFieldSpec[];
}

/** Add menu groups of the block catalog (spec 6.2). */
export type HomebrewBlockGroup =
  | "basic" | "abilities" | "spells" | "uses" | "attacks" | "defence" | "checks" | "effects" | "coreRules";

/** What a `name` field names: picked from the names in scope (pools, toggles, auras, selects, named choices,
 * extra damage, features, classes, spell lists, spells). */
export type HomebrewNameScope =
  | "resource" | "toggle" | "aura" | "select" | "choice" | "extraDamage" | "feature" | "class" | "spellList" | "spell";

/** One shape of a `variant` block field (`resource` maximum: fixed, statistic, class table, formula, rolled). */
export interface HomebrewBlockVariant {
  /** The `type` value. */
  value: string;
  label: string;
  /** The object's other parts. */
  fields: HomebrewBlockField[];
}

/** One field of a block dialog (spec 6.2; WB-05). `key` is the rule field (or the part's key inside its parent);
 * `bindings[0]` is its path in the rule (`max.expr`); `tab` is `rules`. */
export interface HomebrewBlockField extends HomebrewFieldSpec {
  /** `formula` / `formulas`: the dialect of the field's engine reader (offer only its terms). */
  dialect?: HomebrewFormulaDialect;
  /** Free text the engine shows verbatim; `help` is "Shown as a note; Borealis does not apply it." */
  shownAsNote?: boolean;
  /** `name`: what it names. */
  scope?: HomebrewNameScope;
  /** `name`: several names. */
  multiple?: boolean;
  /** `group` and `rows`: the parts; `map`: [key, value]. */
  fields?: HomebrewBlockField[];
  /** `variant`: the shapes. */
  variants?: HomebrewBlockVariant[];
  /** `blocks`: the block kinds the nested list holds (a toggle's effects). */
  kinds?: string[];
}

/** One rule block kind of the block catalog (spec 6.1-6.2; WB-05, `borealis_authoring::blocks`). */
export interface HomebrewBlockSpec {
  /** `grant`, `select`, `list` (a select of type List), `stat`, a custom rule kind (`resource`, `grant_spell`...), or the
   * toggle effect `speed_multiply`. */
  kind: string;
  /** Plain words ("Gives a spell"). */
  label: string;
  /** Add-menu group. */
  group: HomebrewBlockGroup;
  /** Its heading ("Uses & resources"). */
  groupLabel: string;
  /** Listed first in the Add menu. */
  common: boolean;
  /** Hidden unless "Show core-rule blocks" (expert). */
  coreOnly: boolean;
  /** Offered only inside a toggle's effects. */
  effectOnly: boolean;
  /** The dialog shows "From level" (`level`). */
  levelGated: boolean;
  /** The dialog shows "Only if" (`requirements`, the condition builder). */
  onlyIf: boolean;
  /** The block's dialog fields: exactly the fields the engine reads, never an unmodelled one. */
  fields: HomebrewBlockField[];
}

/** One statistic of the statistic picker (spec 5.2; catalog owned by WB-05). */
export interface HomebrewStatEntry {
  /** The statistic name the `stat` rule writes (never shown). */
  name: string;
  /** Plain words ("Darkvision range"). */
  label: string;
  /** Class-scoped names (`<class>:spellcasting:slots:N`) are written from a class picker; this is the pattern. */
  classScoped?: boolean;
}

/** A group of the statistic picker ("Abilities", "Armor Class"...). */
export interface HomebrewStatGroup {
  id: string;
  label: string;
  stats: HomebrewStatEntry[];
}

/** NEW PACK (spec 14.1). */
export interface HomebrewPackRequest {
  name: string;
  /** Pack code: 2-8 of `A-Z0-9`, unique among loaded source abbreviations; becomes the id prefix (spec 5.3). */
  code: string;
  /** Pack edition: `dnd2014`, `dnd2024` or `any` (Both editions, IM17). */
  edition: BookRuleset;
  author?: string;
  description?: string;
  /** `none` ("All rights reserved"), `CC-BY-4.0`, `CC0`, or custom text. */
  license?: string;
}

/** Pack settings change (spec 13.10). The code never changes once the pack exists (ids depend on it). */
export interface HomebrewPackPatch {
  name?: string;
  edition?: BookRuleset;
  author?: string;
  description?: string;
  license?: string;
}

/** Problem counts of an element or a draft. */
export interface HomebrewProblemCounts {
  errors: number;
  warnings: number;
}

/** One row of the element list (spec 13.2). */
export interface HomebrewElementSummary {
  id: string;
  name: string;
  kind: HomebrewKindId;
  elementType: string;
  /** The parent element (a feature's class), else null. */
  parentId: string | null;
  /** The element's own edition; null = the pack's. */
  edition: Ruleset | null;
  problems: HomebrewProblemCounts;
  /** An unsaved draft of this element exists on this device. */
  hasDraft: boolean;
}

/** A reference held by a form field: the id travels, the name is shown (spec 5.1). */
export interface HomebrewRef {
  id: string;
  /** Last known name ("Missing: <name>" when `missing`). */
  name: string;
  /** Element type. */
  type: string;
  /** The element is not loaded any more. */
  missing?: boolean;
}

/** One rule block of a form (spec 6; WB-04 form, WB-05 catalog). */
export interface HomebrewBlock {
  /** Stable key of the block inside the draft (UI lists, undo); never written to content. A new block, or a block
   * whose kind changes, needs a new key. */
  key: string;
  /** The `HomebrewBlockSpec.kind` of its dialog: `grant`, `select`, `list` (a select of type List), `stat`, a custom
   * kind, `speed_multiply` (only inside a toggle's effects); or the kind of a rule kept whole (`unknown`, or a kind
   * the catalog doesn't know: no fields, one `preserved` part). */
  kind: string;
  /** The block as one plain sentence ("From level 5: lets the player choose 2 Eldritch Invocations"). The backend
   * rewrites it from the values after each update. */
  sentence: string;
  /** The block dialog's values, by field key: each the value of its field's control (`HomebrewBlockValue`), plus the
   * gate fields `level` and `requirements`. Absent = not set. */
  fields: Record<string, unknown>;
  /** Parts of the rule the block dialog does not edit, kept byte-for-byte (read-only, removable; WB-04). */
  preserved?: HomebrewPreservedPart[];
}

/* ----------------------------------------------------------------------------------------------------------------
 * Expression builders (spec 6.3, 6.5; owned by WB-02, Rust `borealis_authoring::expr_build`). A form field or block
 * field edited with the `condition`, `tagFilter`, `equipped` or `formula` control holds a `HomebrewExprValue`. Ids and
 * the text travel but are never shown (spec 5.1); the UI edits `value.model` and sends it back with `original`
 * unchanged, and the backend writes the original text while the model still means the same, else canonical text.
 * `ui/src/api/__fixtures__/homebrew-expr-builders.json` (written by the Rust test `expr_dto.rs`) holds a sample of
 * every variant.
 * ------------------------------------------------------------------------------------------------------------- */

/** A builder model, or text the builder cannot represent, shown as a read-only "kept as is" chip with Remove. */
export type HomebrewBuilt<M> =
  | { shape: "model"; model: M }
  | { shape: "kept"; text: string; reason: string };

/** One expression field: the builder state and the stored text it was read from (absent for a new value). */
export interface HomebrewExprValue<M> {
  value: HomebrewBuilt<M>;
  original?: string;
}

/** A problem with a builder value (`path` inside the model, `items.1.min`; empty = the whole value). */
export interface HomebrewBuilderProblem {
  path: string;
  message: string;
}

export type HomebrewGroupMode = "all" | "any" | "none";

/** All of / Any of / None of tree of rows; an empty group writes nothing. A parsed root is always a group. */
export type HomebrewExprNode<R> =
  | { node: "row"; row: R }
  | { node: "group"; mode: HomebrewGroupMode; items: HomebrewExprNode<R>[] };

export type HomebrewAbility = "strength" | "dexterity" | "constitution" | "intelligence" | "wisdom" | "charisma";

/** Condition builder rows (`requirements`). `class` and `stat` are lower case. */
export type HomebrewConditionRow =
  | { row: "has"; id: string }
  | { row: "lacks"; id: string }
  | { row: "ability"; ability: HomebrewAbility; min: number }
  | { row: "characterLevel"; min: number }
  | { row: "level"; min: number }
  | { row: "classLevel"; class: string; min: number }
  | { row: "feature"; name: string }
  | { row: "spellcasting"; level: number }
  | { row: "type"; value: string }
  | { row: "statistic"; stat: string; min: number }
  | { row: "kept"; text: string };

export type HomebrewCondition = HomebrewExprNode<HomebrewConditionRow>;

/** Tag filter rows (`supports`): a tag, an element, the class's spell list, a level the class can cast, the value of a
 * named choice (`select_param`), or a placeholder kept as is. */
export type HomebrewTagRow =
  | { row: "tag"; tag: string }
  | { row: "element"; id: string }
  | { row: "classList" }
  | { row: "castableLevel" }
  | { row: "choice"; name: string }
  | { row: "kept"; text: string };

/** `specific`: Specific elements mode (single bars); it has a meaning only with an Any of group. */
export interface HomebrewTagFilter {
  specific: boolean;
  root: HomebrewExprNode<HomebrewTagRow>;
}

export type HomebrewSlot = "armor" | "shield" | "mainHand" | "offHand";

/** What a slot holds. Armor: any, none, light, medium, heavy, item; shield: any, none, item; main hand: any, none,
 * weapon, versatile, item; off hand: any, none, weapon, item. `item` is an id or name in lower case. */
export type HomebrewSlotTest =
  | { kind: "any" }
  | { kind: "none" }
  | { kind: "light" }
  | { kind: "medium" }
  | { kind: "heavy" }
  | { kind: "weapon" }
  | { kind: "versatile" }
  | { kind: "item"; item: string };

/** "While wearing/holding" rows (`equipped`). */
export type HomebrewEquippedRow =
  | { row: "slot"; slot: HomebrewSlot; test: HomebrewSlotTest }
  | { row: "attuned"; item: string }
  | { row: "wearing"; item: string }
  | { row: "kept"; text: string };

export type HomebrewEquipped = HomebrewExprNode<HomebrewEquippedRow>;

export type HomebrewFormulaOp = "add" | "sub" | "mul" | "div";

/** A formula. `halfProficiency` is offered only in the `value` dialect, `classSaveDc` only in `saveDc`. */
export type HomebrewFormulaNode =
  | { term: "number"; value: number }
  | { term: "proficiency" }
  | { term: "halfProficiency" }
  | { term: "ability"; ability: HomebrewAbility }
  | { term: "classLevel"; class: string }
  | { term: "classSaveDc"; class: string }
  | { term: "binary"; op: HomebrewFormulaOp; left: HomebrewFormulaNode; right: HomebrewFormulaNode }
  | { term: "max"; args: HomebrewFormulaNode[] }
  | { term: "min"; args: HomebrewFormulaNode[] }
  | { term: "floor"; arg: HomebrewFormulaNode }
  | { term: "ceilDiv"; num: HomebrewFormulaNode; den: HomebrewFormulaNode };

/** Which engine reader a formula field is read by, so which terms its builder offers (spec 6.3; the field table is
 * `borealis_authoring::expr_build::formula::FORMULA_FIELDS`). */
export type HomebrewFormulaDialect = "formula" | "saveDc" | "breathDc" | "noteDc" | "value" | "valueOrClassLevel";

/* ----------------------------------------------------------------------------------------------------------------
 * Block field values (spec 6; the WB-04/WB-05 join, Rust `borealis_authoring::form::CatalogBlockCodec`). The value
 * of a key of `HomebrewBlock.fields` is the value of its block field's control (`HomebrewBlockField.control`); the
 * gate fields are `level` (`number`, 1-20, "From level") and `requirements` (`condition`, "Only if"). A field that is
 * absent (or null) is not set. The sample form in `api/__fixtures__/homebrew-form.json` has a block of each shape.
 * ---------------------------------------------------------------------------------------------------------------- */

/** Free casts per rest (`uses`): a number, `"proficiency"`, an ability (its modifier, at least 1), or a class table
 * column. */
export type HomebrewUsesValue = number | "proficiency" | HomebrewAbility | { type: "table"; key: string };

/** `abilitySource`: an ability, or the ability chosen in a named choice. */
export type HomebrewAbilitySourceValue = HomebrewAbility | { param: string };

/** `abilityChoice`: any N different abilities, or a list of abilities. */
export type HomebrewAbilityChoiceValue = number | HomebrewAbility[];

/** `numberOrStatistic`: a number or a statistic. */
export type HomebrewNumberOrStatisticValue = number | { type: "stat"; stat: string };

/** The value a block field holds, by its control. `group`, `rows`, `variant` and `map` hold objects whose parts hold
 * the values of their own controls (`HomebrewBlockField.fields` / `variants`); `blocks` holds nested blocks (a
 * toggle's effects: never with `preserved`). */
export interface HomebrewBlockValueByControl {
  auto: unknown;
  text: string;
  multiline: string;
  /** A number; with `choices`, also one of those words (`any`). */
  number: number | string;
  numbers: number[];
  check: boolean;
  combo: string;
  chips: string[];
  /** The id travels, the name is shown. */
  reference: HomebrewRef;
  references: HomebrewRef[];
  /** A name in `scope`; a list when `multiple`. */
  name: string | string[];
  statistic: string;
  /** `"2"`, `"wisdom:modifier"`, `"-dexterity:modifier"`. */
  statValue: string;
  numberOrStatistic: HomebrewNumberOrStatisticValue;
  condition: HomebrewExprValue<HomebrewCondition>;
  tagFilter: HomebrewExprValue<HomebrewTagFilter>;
  equipped: HomebrewExprValue<HomebrewEquipped>;
  formula: HomebrewExprValue<HomebrewFormulaNode>;
  formulas: HomebrewExprValue<HomebrewFormulaNode>[];
  /** Dice text (`1d10`). */
  dice: string;
  uses: HomebrewUsesValue;
  abilitySource: HomebrewAbilitySourceValue;
  abilityChoice: HomebrewAbilityChoiceValue;
  /** Point splits (`[[2, 1], [1, 1, 1]]`). */
  splits: number[][];
  /** A column of the class table (`rages`). */
  classCounter: string;
  group: Record<string, unknown>;
  rows: Record<string, unknown>[];
  /** `type` is one of `variants`' values. */
  variant: { type: string } & Record<string, unknown>;
  map: Record<string, unknown>;
  blocks: HomebrewBlock[];
}

/** The value of a block field edited with `control`. */
export type HomebrewBlockValue<C extends keyof HomebrewBlockValueByControl> = HomebrewBlockValueByControl[C];

/* ----------------------------------------------------------------------------------------------------------------
 * Form field values (spec 3.2-3.4, 15.1; owned by WB-04, Rust `borealis_authoring::form::value`). The value of a field
 * in `HomebrewForm.fields`, by control: `text`/`multiline`/`combo` a string (`""` = not set), `check`/`toggle` a boolean,
 * `tags`/`chips` a string list, `reference` a `HomebrewRef` or null, `condition` a `HomebrewExprValue<HomebrewCondition>`,
 * `richText` the description tree (`DocJson`, `pages/homebrew/rich/types.ts`), `sheet` a `HomebrewSheetValue` or null,
 * `setters` a `HomebrewSetterRow[]`, `mapsTo` a `HomebrewMapsToValue` or null. A sample of each is in
 * `api/__fixtures__/homebrew-form.json`, written by `crates/borealis-authoring/tests/form_dto.rs`.
 * ---------------------------------------------------------------------------------------------------------------- */

/** One row of the expert "Other properties" list (spec 4.12): a setter's name, value and attributes. */
export interface HomebrewSetterRow {
  name: string;
  value: string;
  attributes: Record<string, string>;
}

/** One sheet line: text with `{{stat}}` placeholders, from a level (absent = 1). */
export interface HomebrewSheetLine {
  text: string;
  level?: number | null;
  usage?: string | null;
  action?: string | null;
  /** Unknown attributes of the stored line: send back unchanged. */
  kept?: unknown;
}

/** "On the character sheet" (spec 3.2). */
export interface HomebrewSheetValue {
  /** Listed on the sheet; null = yes. */
  display: boolean | null;
  /** Short name. */
  alt: string | null;
  /** "1/Long Rest". */
  usage: string | null;
  action: string | null;
  lines: HomebrewSheetLine[];
}

/** "2014/2024 counterpart" (expert). */
export interface HomebrewMapsToValue {
  dnd2014: HomebrewRef | null;
  dnd2024: HomebrewRef | null;
  kind: "equivalent" | "approximate" | "none" | null;
}

/** A part of the element the form does not edit, written back byte-for-byte (spec 3.4). */
export interface HomebrewPreservedPart {
  /** Stable key (for Remove). */
  key: string;
  /** Plain words ("An unknown XML attribute `foo`"). */
  label: string;
}

/**
 * The editable state of one draft (spec 15.1 `FormState`). `fields` holds a value per `HomebrewFieldSpec.key` of the kind:
 * text, numbers, booleans, string lists, `HomebrewRef`s, and the builder/rich-text states (owned by WB-02, WB-03).
 */
export interface HomebrewForm {
  /** Stable key of the form inside the draft (`e`, a child `e/0`); children are matched by it. A new child needs a new
   * key. The element id is never sent: the backend keeps it. */
  key?: string;
  kind: HomebrewKindId;
  fields: Record<string, unknown>;
  blocks: HomebrewBlock[];
  /** Child elements edited inside this one (a class's features), each its own form. */
  children: HomebrewForm[];
  /** Kept from the original (read-only, removable). */
  preserved: HomebrewPreservedPart[];
}

export type HomebrewProblemSeverity = "error" | "warning" | "info";

/** One validation finding, as a plain sentence pointing at its field (spec 8.2-8.3). */
export interface HomebrewProblem {
  severity: HomebrewProblemSeverity;
  message: string;
  /** Form path of the field (`fields.level`, `blocks.2.fields.number`, `children.0.fields.name`); null = the element. */
  path: string | null;
  /** The element it belongs to (a class draft has features), else null. */
  elementId: string | null;
  /** Machine-readable code (an `IssueCode`, a rule problem, `sweep`, `engine`, `lint.*`, `balance.*`). */
  code: string;
}

/** DATA sub-tab (spec 12.1): the stored element(s), read-only. Tree shape owned by WB-23. */
export interface HomebrewGeneratedView {
  /** The canonical element JSON, one per element of the draft, for "Copy as JSON". */
  json: string;
  /** The indented `key: value` tree. */
  tree: Record<string, unknown>[];
}

/** An open draft (spec 10.1). */
export interface HomebrewDraftView {
  draftId: string;
  packId: string;
  /** The saved element it edits; null for a new element until its first save. */
  elementId: string | null;
  form: HomebrewForm;
  /** The live description pane, exactly as the builder shows it. */
  preview: ElementDetail | null;
  problems: HomebrewProblem[];
  generated: HomebrewGeneratedView;
  /** ISO time of the last autosave. */
  updatedAt: string;
  /** The saved element changed since the draft started (another device's pack arrived): ask Keep my draft / Use the saved version. */
  baseChanged: boolean;
}

/** Drafts on this device (spec 10.1: "Unsaved changes from <time>"). */
export interface HomebrewDraftSummary {
  draftId: string;
  packId: string;
  elementId: string | null;
  kind: HomebrewKindId;
  name: string;
  updatedAt: string;
}

/** Save result. */
export interface HomebrewSaveResult {
  pack: ContentPack;
  /** Ids written (the element and its children). */
  elementIds: string[];
}

/** A local character that uses an element (delete scan, spec 10.3). */
export interface HomebrewCharacterRef {
  characterId: string;
  name: string;
}

/** Delete: without `confirm`, a scan; with it (or when no character uses the elements), the delete. */
export interface HomebrewDeleteResult {
  deleted: boolean;
  usedBy: HomebrewCharacterRef[];
  /** The pack after the delete; null when nothing was deleted. */
  pack: ContentPack | null;
}

/** Multi-select edits (spec 12.5): one undoable step, one save. */
export interface HomebrewBulkPatch {
  edition?: Ruleset | null;
  addTags?: string[];
  removeTags?: string[];
  /** Move the elements to another homebrew pack. */
  moveToPackId?: string;
}

/** Filters of a reference picker (spec 5.1, `PickerFilterBar` + `EditionSegments`; spells add the Magic filter bar). */
export interface HomebrewPickerFilters {
  sources?: string[];
  edition?: Ruleset | null;
  spellSchool?: string;
  spellLevels?: number[];
  ritual?: boolean;
  concentration?: boolean;
}

/** Paging of a picker query. */
export interface HomebrewPageRequest {
  offset: number;
  limit: number;
}

/** One picker row: NAME / summary / PREREQUISITES / SOURCE (spec 13.5). */
export interface HomebrewPickerRow {
  id: string;
  name: string;
  type: string;
  summary: string;
  prerequisites: string | null;
  sourceName: string;
  edition: Ruleset | null;
  /** An unsaved draft of the current pack. */
  draft: boolean;
  /** Spells only (WB-10, the spell picker's Magic grid, spec 13.7): level 0-9, school and casting facts. */
  level?: number;
  school?: string;
  spell?: SpellFacts;
}

/** Where-used (spec 12.3). */
export type HomebrewUseRelation = "grants" | "selects" | "requires" | "lists";

export interface HomebrewElementRef {
  id: string;
  name: string;
  type: string;
  /** The pack holding it (null for bundled content shown without a pack). */
  packId: string | null;
  relation: HomebrewUseRelation;
}

/** One spell of a SPELL LISTS grid (spec 12.4). */
export interface HomebrewSpellListRow {
  id: string;
  name: string;
  level: number;
  school: string;
  verbal: boolean;
  somatic: boolean;
  material: boolean;
  sourceName: string;
  /** On the list. */
  onList: boolean;
  /** The pack can remove it (an own spell or an own append). */
  removable: boolean;
}

export interface HomebrewSpellListView {
  /** The list's tag (`Wizard`). */
  listTag: string;
  /** The list belongs to an own class (else additions are appends). */
  own: boolean;
  /** "84 spells on this list". */
  count: number;
  rows: HomebrewSpellListRow[];
}

/** Try it setup (spec 9). Slots left out take the edition's SRD defaults. */
export interface HomebrewTrialSetup {
  edition?: Ruleset;
  level?: number;
  classId?: string;
  subclassId?: string;
  speciesId?: string;
  backgroundId?: string;
  /** Ability scores STR..CHA; default the standard array by the class's primary abilities. */
  abilityScores?: number[];
  /** Class drafts: try as a second class (Fighter 1 / draft N). */
  asSecondClass?: boolean;
}

/** One Try it change: the level, a setup slot, or a choice. */
export interface HomebrewTrialChange {
  level?: number;
  setup?: HomebrewTrialSetup;
  /** Pick `elementId` (or clear with null) in the trial character's slot. */
  slotId?: string;
  elementId?: string | null;
}

/** One rung of the level ladder (spec 9). */
export interface HomebrewLadderRow {
  level: number;
  features: string[];
  choices: string[];
  /** Spell slots per slot level 1-9 at this level. */
  slots: number[];
  problems: HomebrewProblem[];
}

/** The trial character (never saved, never listed). */
export interface HomebrewTrialView {
  trialId: string;
  snapshot: CharacterSnapshot;
  ladder: HomebrewLadderRow[];
  problems: HomebrewProblem[];
  /** Snapshot fields whose values changed because of the draft (`stats.ac`, `abilities.final.2`...). */
  changed: string[];
}
