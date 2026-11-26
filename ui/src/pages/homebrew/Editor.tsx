import { type ReactNode, type RefObject, useEffect, useRef, useState } from "react";
import type { HomebrewDraftView, HomebrewFieldSpec, HomebrewForm, HomebrewKindDescriptor, HomebrewProblem, HomebrewTab, Ruleset } from "../../api/contract";
import { Icon, TabControl, ToggleSwitch } from "../../components";
import { PreservedList } from "./blocks/BlockDialog";
import { CasterSection } from "./caster/CasterSection";
import { SpellcastingTab } from "./caster/SpellcastingTab";
import type { ChildEditing } from "./class/ChildDialog";
import { CreatorEnvProvider } from "./env";
import { DataView } from "./DataView";
import { editionNote, otherEdition, shownForEdition } from "./editions";
import { Band } from "../spells/TwoPane";
import "../equipment/equipment.css";
import { FieldView } from "./FieldView";
import { ProblemsPanel } from "./ProblemsPanel";
import { TryIt } from "./TryIt";
import { PROPERTIES_FIELD, WEAPON_KIND, withWeaponProperties } from "./weaponHands";
import { CLASS_KIND, GROUP_FIELDS, withClassProficiencies } from "./proficiencyGroups";
import { noteStatus } from "./statusNote";
import { NestedPane } from "../builder/DescriptionPane";
import type { DraftController } from "./useDraft";
import { OverrideNotice } from "./Overrides";
import type { AutosaveState } from "./draftSession";

/** The phone's own sub-tab for the live description (spec 16: the description pane becomes PREVIEW). */
export const PREVIEW_TAB: HomebrewTab = { id: "preview", label: "PREVIEW" };
/** The read-only generated-data view every kind has (spec 12.1, 13.3: the last of the editor's sub-tabs). */
export const DATA_TAB: HomebrewTab = { id: "data", label: "DATA" };

const AUTOSAVE_TEXT: Record<AutosaveState, string> = {
  saved: "All changes kept on this device",
  pending: "Unsaved changes",
  saving: "Keeping changes...",
  error: "Changes not kept",
};

export interface EditorActions {
  onSave: () => void;
  /** Opens Try it (spec 9); `onTryItClose` closes it. */
  onTryIt: () => void;
  onTryItClose: () => void;
  onDuplicate: () => void;
  onJump: (problem: HomebrewProblem) => void;
  /** Makes a child of the open element through the backend (`homebrew_new_draft {parentId}`, WB-16): see `ChildEditing`. */
  onNewChild?: ChildEditing["newChild"];
  /** Asks before a saved child is removed when characters use it (spec 10.3): see `ChildEditing`. */
  onConfirmRemove?: ChildEditing["confirmRemove"];
  /** Something wrote the pack outside this draft (class options made from a choice block, spec 4.8): lists reload. */
  onPackChanged?: () => void;
  /** HC20: the open override is saved in the pack (its notice offers Revert), and what follows a Revert. */
  overrideSaved?: boolean;
  onReverted?: () => void;
}

/** A field shown for the form's edition (`HomebrewFieldSpec.editions`; no edition = the pack's, every field shows); a field
 * of the other edition still shows while it holds a value, with a note (it still writes its rules, P2a). */
const forEdition = (f: HomebrewFieldSpec, edition: Ruleset | null, form: HomebrewForm | null = null): boolean =>
  shownForEdition(f, edition, form?.fields[f.key]);

/** Fields of one sub-tab, grouped under their SectionSecondary headings (spec 13.4) in field order. */
function sections(fields: readonly HomebrewFieldSpec[]): [string, HomebrewFieldSpec[]][] {
  const out: [string, HomebrewFieldSpec[]][] = [];
  for (const f of fields) {
    const name = f.section ?? "";
    const last = out[out.length - 1];
    if (last && last[0] === name) out[out.length - 1] = [name, [...last[1], f]];
    else out.push([name, [f]]);
  }
  return out;
}

/** The element's own edition, when it has one (the description toolbar's higher-level paragraph follows it). */
export function editionOf(form: HomebrewForm | null): Ruleset | null {
  const e = form?.fields.edition;
  return e === "dnd2014" || e === "dnd2024" ? e : null;
}

/** A feat's "Prerequisite as shown": left empty or as it was read, the save writes it from the condition (WB-13). */
const isWrittenPrerequisite = (f: HomebrewFieldSpec, form: HomebrewForm): boolean => form.kind === "feat" && f.key === "prerequisiteText";

/** A field's value: a `blocks` field is the form's block list (spec 6), a read-only one the backend's last answer (a
 * subclass's tag and levels come from its class, WB-18), every other field its `fields` entry. A feat's prerequisite
 * sentence, while not edited since the last autosave, is what the save writes: the live preview's, so it follows the
 * condition as it is edited; typed over, it is the author's text. */
const valueOf = (f: HomebrewFieldSpec, form: HomebrewForm, view: HomebrewDraftView | null | undefined): unknown => {
  const answer = view?.form;
  if (f.control === "blocks") return form.blocks;
  if (f.readOnly && answer && f.key in answer.fields) return answer.fields[f.key];
  if (isWrittenPrerequisite(f, form) && view?.preview && answer && form.fields[f.key] === answer.fields[f.key]) return view.preview.prerequisites ?? "";
  return form.fields[f.key];
};

/** The SPELLCASTING sub-tab (spec 4.6): a class's edits its Spellcasting feature; any other form's, its own section. */
const SPELLCASTING_TAB = "spellcasting";

/** The sticky head's height as `--hb-head-h` on the editor (phones): the rich-text toolbar sticks under it while editing. */
function useHeadHeight(head: RefObject<HTMLDivElement>, editor: RefObject<HTMLElement>, on: boolean): void {
  useEffect(() => {
    const h = head.current;
    const e = editor.current;
    if (!on || !h || !e || typeof ResizeObserver === "undefined") return;
    const set = () => e.style.setProperty("--hb-head-h", `${h.offsetHeight}px`);
    set();
    const ro = new ResizeObserver(set);
    ro.observe(h);
    return () => ro.disconnect();
  }, [head, editor, on]);
}

/** The kind's sub-tabs as the band shows them; phones add PREVIEW after the kind's own. */
export function editorTabs(kind: HomebrewKindDescriptor, narrow: boolean, edition: Ruleset | null = null, form: HomebrewForm | null = null): HomebrewTab[] {
  // a tab whose fields are all of the other edition is not shown (a 2024 background has no CHARACTERISTICS, WB-14), unless
  // one of them still holds a value
  const shown = kind.tabs.filter((t) => {
    const own = kind.fields.filter((f) => f.tab === t.id);
    return own.length === 0 || own.some((f) => forEdition(f, edition, form));
  });
  return narrow ? [...shown, DATA_TAB, PREVIEW_TAB] : [...shown, DATA_TAB];
}

/**
 * The element editor (spec 13.2-13.4): the title band, the command row (undo, redo, save, try it, duplicate), PROBLEMS (n),
 * the 30 px sub-tab band, then the current sub-tab's form in the Manage > Character form layout.
 */
export function Editor({ kind, kinds = [kind], draft, tab, onTab, narrow, preview, actions, tryItOpen = false }: {
  kind: HomebrewKindDescriptor; draft: DraftController; tab: string; onTab: (id: string) => void; narrow: boolean; preview: ReactNode; actions: EditorActions;
  /** Every kind (a child's form inside this one, WB-16). */
  kinds?: readonly HomebrewKindDescriptor[];
  /** Try it is open (spec 9, 13.9: a flyout over the editor). */
  tryItOpen?: boolean;
}) {
  const [expert, setExpert] = useState(false);
  const headRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<HTMLElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  useHeadHeight(headRef, editorRef, narrow && draft.form !== null);
  // Desktop: another sub-tab opens at its top, as an Aurora sub-tab does (the form's scroller is shared by the tabs, so it
  // kept the last tab's scroll, WB-26)
  useEffect(() => { if (!narrow && panelRef.current) panelRef.current.scrollTop = 0; }, [tab, narrow]);
  const form = draft.form;
  if (!form) return null;
  const name = typeof form.fields.name === "string" && form.fields.name.trim() ? form.fields.name.trim() : `New ${kind.label.toLowerCase()}`;
  const edition = editionOf(form);
  const tabs = editorTabs(kind, narrow, edition, form);
  const current = tabs.some((t) => t.id === tab) ? tab : tabs[0]?.id ?? "";
  // the class table is a part of the LEVELS grid (WB-16)
  const fields = kind.fields.filter((f) => f.tab === current && (expert || !f.expert) && f.control !== "classTable" && forEdition(f, edition, form));
  const hasExpert = kind.fields.some((f) => f.expert);
  const setValue = (f: HomebrewFieldSpec, v: unknown, typing: boolean) => {
    if (f.control === "blocks") draft.edit((cur) => ({ ...cur, blocks: v as HomebrewForm["blocks"] }));
    // a weapon's slot follows its Two-Handed property, in the same undo step (P3)
    else if (form.kind === WEAPON_KIND && f.key === PROPERTIES_FIELD) draft.edit((cur) => ({ ...cur, fields: withWeaponProperties(cur.fields, v) }));
    // a class lists its proficiencies by kind: one picked under another kind goes to its own at once, and says so (P3)
    else if (form.kind === CLASS_KIND && GROUP_FIELDS.includes(f.key) && withClassProficiencies(form.fields, f.key, v).moved.length) {
      const label = (key: string) => kind.fields.find((x) => x.key === key)?.label ?? key;
      const { moved } = withClassProficiencies(form.fields, f.key, v);
      draft.edit((cur) => ({ ...cur, fields: withClassProficiencies(cur.fields, f.key, v).fields }));
      noteStatus(moved.map((m) => `${m.name} is listed under ${label(m.to)}: a class lists its proficiencies by kind.`).join(" "));
    } else draft.setField(f.key, v, typing);
  };
  const expertSwitch = <span className="hb-expert"><ToggleSwitch label="Expert fields" checked={expert} onChange={setExpert} onLabel="Expert" offLabel="Expert" /></span>;
  const childEditing: ChildEditing = {
    form, kinds, problems: draft.problems, edition,
    edit: (next, group) => draft.edit(next, group ?? null),
    newChild: actions.onNewChild ?? (async () => null),
    ...(actions.onConfirmRemove ? { confirmRemove: actions.onConfirmRemove } : {}),
  };
  const removePreserved = (key: string) => draft.edit((cur) => ({ ...cur, preserved: cur.preserved.filter((p) => p.key !== key) }));
  // Spec 16 / PH2: on phones the same row is pinned above the bottom tabs, so it follows the form (a sticky row sticks
  // only below its own content). Order as spec 16 lists it: Undo, Redo, Try it, Save.
  const commands = (
    <div className={narrow ? "eq-inv-actions hb-commands eq-pin" : "eq-inv-actions hb-commands"} role="toolbar" aria-label="Element commands">
      <button type="button" aria-label="Undo" title="Undo (Ctrl+Z)" disabled={!draft.canUndo} onClick={draft.undo}><Icon name="undo" />Undo</button>
      <button type="button" aria-label="Redo" title="Redo (Ctrl+Shift+Z)" disabled={!draft.canRedo} onClick={draft.redo}><Icon name="redo" />Redo</button>
      <button type="button" aria-label="Try it" title="Try it (Ctrl+T)" disabled={!draft.view} onClick={actions.onTryIt}><Icon name="dice" />Try it</button>
      <button type="button" aria-label="Save" title="Save (Ctrl+S)" disabled={draft.saving} onClick={actions.onSave}><Icon name="save" />Save</button>
      {!narrow && <button type="button" aria-label="Duplicate" title="Duplicate (Ctrl+D)" disabled={!draft.view?.elementId} onClick={actions.onDuplicate}><Icon name="copy" />Duplicate</button>}
    </div>
  );
  const band = <Band title={name}><span className={`hb-autosave hb-${draft.autosave}`} role="status">{AUTOSAVE_TEXT[draft.autosave]}</span></Band>;
  // Desktop (U7, WB-26): the band holds only the sub-tabs, in rows of the band's height when a kind has more than fit
  // (a class has nine); the Expert switch sits at the right end of the command row, as the Equipment filter row's
  // MANAGE COINS does, so it never covers a tab.
  const tabBand = (
    <div className="aur-subband hb-band">
      <TabControl tabs={tabs.map((t) => ({ id: t.id, label: t.label }))} active={current} onChange={onTab} label="Editor sections" idPrefix="hb-tab" />
    </div>
  );
  const error = draft.error && <p className="hb-error-line" role="alert">{draft.error}</p>;
  const problemsPanel = <ProblemsPanel problems={draft.problems} checking={draft.checking} onJump={actions.onJump} onOpen={() => void draft.check()} />;
  // a deep class copy saves its subclasses with it (spec 11): they are not on screen, so the editor names them
  const companionNames = draft.view?.savedWith ?? [];
  const savedWith = companionNames.length > 0
    ? <p className="hb-saved-with" role="note" aria-label="Saved with it">Saved with it: {companionNames.join(", ")}.</p>
    : null;
  // HC20: an override says what saving does, and when its original changed underneath or is not loaded (spec 22.6)
  const ov = draft.view?.override;
  const overrideNotice = ov && draft.view
    ? <OverrideNotice info={ov} packId={draft.view.packId} saved={actions.overrideSaved ?? false} onReverted={actions.onReverted ?? (() => undefined)} />
    : null;
  return (
    <CreatorEnvProvider draftId={draft.view?.draftId ?? null} packId={draft.view?.packId ?? null} onPackChanged={actions.onPackChanged} edition={edition} form={form} answer={draft.view?.form ?? null} problems={draft.problems} expert={expert}>
      <section className="hb-editor" aria-label="Element editor" ref={editorRef}>
        {narrow ? (
          // Phones (spec 16): the element's name and the sub-tab strip stay at the top while the form scrolls under them;
          // PROBLEMS follows the strip, and the commands are pinned at the bottom.
          <>
            <div className="hb-editor-head" ref={headRef}>{band}{tabBand}</div>
            {error}
            {overrideNotice}
            {savedWith}
            {problemsPanel}
            {hasExpert && <div className="hb-expert-row">{expertSwitch}</div>}
          </>
        ) : (
          // Desktop (13.2-13.4): the title band, commands, PROBLEMS and the sub-tab band stay; only the form scrolls.
          <>{band}<div className="hb-cmdrow">{commands}{hasExpert && expertSwitch}</div>{error}{overrideNotice}{savedWith}{problemsPanel}{tabBand}</>
        )}
        <div role="tabpanel" id="hb-tab-panel" aria-labelledby={`hb-tab-${current}`} className="hb-tabpanel" ref={panelRef}>
          {current === PREVIEW_TAB.id ? <NestedPane.Provider value>{preview}</NestedPane.Provider> : current === DATA_TAB.id ? <DataView generated={draft.view?.generated} /> : current === SPELLCASTING_TAB && (kind.id === "class" || kind.id === "subclass")
            ? <SpellcastingTab editing={childEditing} expert={expert} />
            : current === SPELLCASTING_TAB
              ? <CasterSection fields={fields} form={form} problems={draft.problems} edition={edition} expert={expert}
                onFields={(patch, group) => draft.edit((cur) => ({ ...cur, fields: { ...cur.fields, ...patch } }), group)} />
              : fields.length === 0
            ? <p className="hb-pending">Nothing to fill in here yet.</p>
            : sections(fields).map(([title, list], i) => (
              <div key={`${i}-${title}`} className="hb-section">
                {title && <h3 className="hb-section-title">{title}</h3>}
                <div className="mg-grid two">
                  {list.map((f) => (
                    <FieldView key={f.key} field={f} value={valueOf(f, form, draft.view)} problems={draft.problems} kindId={kind.id} childEditing={childEditing}
                      note={otherEdition(f, edition) ? editionNote(f) : undefined} onChange={(v, typing) => setValue(f, v, typing)} />
                  ))}
                </div>
              </div>
            ))}
          {current === tabs[0]?.id && current !== DATA_TAB.id && <PreservedList parts={form.preserved} onRemove={removePreserved} />}
        </div>
        {narrow && commands}
      </section>
      {tryItOpen && <TryIt kind={kind} draft={draft} onClose={actions.onTryItClose} onJump={actions.onJump} />}
    </CreatorEnvProvider>
  );
}
