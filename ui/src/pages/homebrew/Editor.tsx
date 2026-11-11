import { type ReactNode, useState } from "react";
import type { HomebrewFieldSpec, HomebrewForm, HomebrewKindDescriptor, HomebrewProblem, HomebrewTab, Ruleset } from "../../api/contract";
import { Icon, TabControl, ToggleSwitch } from "../../components";
import { PreservedList } from "./blocks/BlockDialog";
import { CasterSection } from "./caster/CasterSection";
import { SpellcastingTab } from "./caster/SpellcastingTab";
import type { ChildEditing } from "./class/ChildDialog";
import { CreatorEnvProvider } from "./env";
import { Band } from "../spells/TwoPane";
import "../equipment/equipment.css";
import { FieldView } from "./FieldView";
import { ProblemsPanel } from "./ProblemsPanel";
import { TryIt } from "./TryIt";
import type { DraftController } from "./useDraft";
import type { AutosaveState } from "./draftSession";

/** The phone's own sub-tab for the live description (spec 16: the description pane becomes PREVIEW). */
export const PREVIEW_TAB: HomebrewTab = { id: "preview", label: "PREVIEW" };

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
}

/** A field shown for the form's edition (`HomebrewFieldSpec.editions`; no edition = the pack's, every field shows). */
const forEdition = (f: HomebrewFieldSpec, edition: Ruleset | null): boolean => !f.editions || !edition || f.editions.includes(edition);

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

/** A field's value: a `blocks` field is the form's block list (spec 6), a read-only one the backend's last answer (a
 * subclass's tag and levels come from its class, WB-18), every other field its `fields` entry. */
const valueOf = (f: HomebrewFieldSpec, form: HomebrewForm, answer: HomebrewForm | null | undefined): unknown => {
  if (f.control === "blocks") return form.blocks;
  if (f.readOnly && answer && f.key in answer.fields) return answer.fields[f.key];
  return form.fields[f.key];
};

/** The SPELLCASTING sub-tab (spec 4.6): a class's edits its Spellcasting feature; any other form's, its own section. */
const SPELLCASTING_TAB = "spellcasting";

/** The kind's sub-tabs as the band shows them; phones add PREVIEW after the kind's own. */
export function editorTabs(kind: HomebrewKindDescriptor, narrow: boolean, edition: Ruleset | null = null): HomebrewTab[] {
  // a tab whose fields are all of the other edition is not shown (a 2024 background has no CHARACTERISTICS, WB-14)
  const shown = kind.tabs.filter((t) => {
    const own = kind.fields.filter((f) => f.tab === t.id);
    return own.length === 0 || own.some((f) => forEdition(f, edition));
  });
  return narrow ? [...shown, PREVIEW_TAB] : shown;
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
  const form = draft.form;
  if (!form) return null;
  const name = typeof form.fields.name === "string" && form.fields.name.trim() ? form.fields.name.trim() : `New ${kind.label.toLowerCase()}`;
  const edition = editionOf(form);
  const tabs = editorTabs(kind, narrow, edition);
  const current = tabs.some((t) => t.id === tab) ? tab : tabs[0]?.id ?? "";
  // the class table is a part of the LEVELS grid (WB-16)
  const fields = kind.fields.filter((f) => f.tab === current && (expert || !f.expert) && f.control !== "classTable" && forEdition(f, edition));
  const hasExpert = kind.fields.some((f) => f.expert);
  const setValue = (f: HomebrewFieldSpec, v: unknown, typing: boolean) => {
    if (f.control === "blocks") draft.edit((cur) => ({ ...cur, blocks: v as HomebrewForm["blocks"] }));
    else draft.setField(f.key, v, typing);
  };
  const expertSwitch = <span className="hb-expert"><ToggleSwitch label="Expert fields" checked={expert} onChange={setExpert} onLabel="Expert" offLabel="Expert" /></span>;
  const childEditing: ChildEditing = {
    form, kinds, problems: draft.problems, edition,
    edit: (next, group) => draft.edit(next, group ?? null),
    newChild: actions.onNewChild ?? (async () => null),
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
  return (
    <CreatorEnvProvider draftId={draft.view?.draftId ?? null} edition={edition} form={form} answer={draft.view?.form ?? null} problems={draft.problems} expert={expert}>
      <section className="hb-editor" aria-label="Element editor">
        <Band title={name}><span className={`hb-autosave hb-${draft.autosave}`} role="status">{AUTOSAVE_TEXT[draft.autosave]}</span></Band>
        {!narrow && commands}
        {draft.error && <p className="hb-error-line" role="alert">{draft.error}</p>}
        <ProblemsPanel problems={draft.problems} checking={draft.checking} onJump={actions.onJump} onOpen={() => void draft.check()} />
        <div className="aur-subband hb-band">
          <TabControl tabs={tabs.map((t) => ({ id: t.id, label: t.label }))} active={current} onChange={onTab} label="Editor sections" idPrefix="hb-tab" />
          {hasExpert && !narrow && expertSwitch}
        </div>
        {hasExpert && narrow && <div className="hb-expert-row">{expertSwitch}</div>}
        <div role="tabpanel" id="hb-tab-panel" aria-labelledby={`hb-tab-${current}`} className="hb-tabpanel">
          {current === PREVIEW_TAB.id ? preview : current === SPELLCASTING_TAB && (kind.id === "class" || kind.id === "subclass")
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
                    <FieldView key={f.key} field={f} value={valueOf(f, form, draft.view?.form)} problems={draft.problems} kindId={kind.id} childEditing={childEditing}
                      onChange={(v, typing) => setValue(f, v, typing)} />
                  ))}
                </div>
              </div>
            ))}
          {current === tabs[0]?.id && <PreservedList parts={form.preserved} onRemove={removePreserved} />}
        </div>
        {narrow && commands}
      </section>
      {tryItOpen && <TryIt kind={kind} draft={draft} onClose={actions.onTryItClose} onJump={actions.onJump} />}
    </CreatorEnvProvider>
  );
}
