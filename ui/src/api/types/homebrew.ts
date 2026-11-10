import type { BookRuleset, ElementDetail, Ruleset } from "./core";
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

/** The control a field is edited with (spec 4, 13.4). Builders and pickers are documented in spec 5-7. */
export type HomebrewControl =
  | "text" | "multiline" | "number" | "combo" | "check" | "toggle" | "tags" | "chips"
  | "reference" | "references" | "statistic" | "condition" | "tagFilter" | "equipped" | "formula"
  | "richText" | "sheet" | "grid" | "blocks" | "children";

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

/** One rule block kind of the block catalog (spec 6.1-6.2; the catalog is owned by WB-05). */
export interface HomebrewBlockSpec {
  /** `grant`, `select`, `list`, `stat`, or a custom rule kind (`resource`, `grant_spell`...). */
  kind: string;
  /** Plain words ("Gives a spell"). */
  label: string;
  /** Add-menu group ("Spells", "Uses & resources"...). */
  group: string;
  /** Listed first in the Add menu. */
  common: boolean;
  /** Hidden unless "Show core-rule blocks" (expert). */
  coreOnly: boolean;
  /** The block takes "From level" (a gate field). */
  levelGated: boolean;
  /** The block's dialog fields (the kind's known fields only, never an unmodelled one). */
  fields: HomebrewFieldSpec[];
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

/** One rule block of a form (spec 6). `fields` hold the block dialog's values; owned by WB-05. */
export interface HomebrewBlock {
  /** Stable key of the block inside the draft (UI lists, undo); never written to content. */
  key: string;
  kind: string;
  /** The block as one plain sentence ("From level 5: lets the player choose 2 Eldritch Invocations"). */
  sentence: string;
  fields: Record<string, unknown>;
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
