import { type ReactNode, useState } from "react";
import type { HomebrewFieldSpec, HomebrewForm, HomebrewKindDescriptor, HomebrewProblem, HomebrewTab, Ruleset } from "../../api/contract";
import { Icon, TabControl, ToggleSwitch } from "../../components";
import { PreservedList } from "./blocks/BlockDialog";
import { CreatorEnvProvider } from "./env";
import { Band } from "../spells/TwoPane";
import "../equipment/equipment.css";
import { FieldView } from "./FieldView";
import { ProblemsPanel } from "./ProblemsPanel";
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
  onTryIt: () => void;
  onDuplicate: () => void;
  onJump: (problem: HomebrewProblem) => void;
}

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
function editionOf(form: HomebrewForm): Ruleset | null {
  const e = form.fields.edition;
  return e === "dnd2014" || e === "dnd2024" ? e : null;
}

/** A field's value: a `blocks` field is the form's block list (spec 6), every other field its `fields` entry. */
const valueOf = (f: HomebrewFieldSpec, form: HomebrewForm): unknown => (f.control === "blocks" ? form.blocks : form.fields[f.key]);

/** The kind's sub-tabs as the band shows them; phones add PREVIEW after the kind's own. */
export function editorTabs(kind: HomebrewKindDescriptor, narrow: boolean): HomebrewTab[] {
  return narrow ? [...kind.tabs, PREVIEW_TAB] : kind.tabs;
}

/**
 * The element editor (spec 13.2-13.4): the title band, the command row (undo, redo, save, try it, duplicate), PROBLEMS (n),
 * the 30 px sub-tab band, then the current sub-tab's form in the Manage > Character form layout.
 */
export function Editor({ kind, draft, tab, onTab, narrow, preview, actions }: {
  kind: HomebrewKindDescriptor; draft: DraftController; tab: string; onTab: (id: string) => void; narrow: boolean; preview: ReactNode; actions: EditorActions;
}) {
  const [expert, setExpert] = useState(false);
  const form = draft.form;
  if (!form) return null;
  const name = typeof form.fields.name === "string" && form.fields.name.trim() ? form.fields.name.trim() : `New ${kind.label.toLowerCase()}`;
  const tabs = editorTabs(kind, narrow);
  const current = tabs.some((t) => t.id === tab) ? tab : tabs[0]?.id ?? "";
  const fields = kind.fields.filter((f) => f.tab === current && (expert || !f.expert));
  const hasExpert = kind.fields.some((f) => f.expert);
  const hasTryIt = kind.tabs.some((t) => t.id === "tryIt");
  const setValue = (f: HomebrewFieldSpec, v: unknown, typing: boolean) => {
    if (f.control === "blocks") draft.edit((cur) => ({ ...cur, blocks: v as HomebrewForm["blocks"] }));
    else draft.setField(f.key, v, typing);
  };
  const expertSwitch = <span className="hb-expert"><ToggleSwitch label="Expert fields" checked={expert} onChange={setExpert} onLabel="Expert" offLabel="Expert" /></span>;
  const removePreserved = (key: string) => draft.edit((cur) => ({ ...cur, preserved: cur.preserved.filter((p) => p.key !== key) }));
  // Spec 16 / PH2: on phones the same row is pinned above the bottom tabs, so it follows the form (a sticky row sticks
  // only below its own content). Order as spec 16 lists it: Undo, Redo, Try it, Save.
  const commands = (
    <div className={narrow ? "eq-inv-actions hb-commands eq-pin" : "eq-inv-actions hb-commands"} role="toolbar" aria-label="Element commands">
      <button type="button" aria-label="Undo" title="Undo (Ctrl+Z)" disabled={!draft.canUndo} onClick={draft.undo}><Icon name="undo" />Undo</button>
      <button type="button" aria-label="Redo" title="Redo (Ctrl+Shift+Z)" disabled={!draft.canRedo} onClick={draft.redo}><Icon name="redo" />Redo</button>
      <button type="button" aria-label="Try it" title="Try it (Ctrl+T)" disabled={!hasTryIt} onClick={actions.onTryIt}><Icon name="dice" />Try it</button>
      <button type="button" aria-label="Save" title="Save (Ctrl+S)" disabled={draft.saving} onClick={actions.onSave}><Icon name="save" />Save</button>
      {!narrow && <button type="button" aria-label="Duplicate" title="Duplicate (Ctrl+D)" disabled={!draft.view?.elementId} onClick={actions.onDuplicate}><Icon name="copy" />Duplicate</button>}
    </div>
  );
  return (
    <CreatorEnvProvider draftId={draft.view?.draftId ?? null} edition={editionOf(form)} form={form} answer={draft.view?.form ?? null} problems={draft.problems} expert={expert}>
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
          {current === PREVIEW_TAB.id ? preview : fields.length === 0
            ? <p className="hb-pending">Nothing to fill in here yet.</p>
            : sections(fields).map(([title, list]) => (
              <div key={title || "main"} className="hb-section">
                {title && <h3 className="hb-section-title">{title}</h3>}
                <div className="mg-grid two">
                  {list.map((f) => (
                    <FieldView key={f.key} field={f} value={valueOf(f, form)} problems={draft.problems} kindId={kind.id}
                      onChange={(v, typing) => setValue(f, v, typing)} />
                  ))}
                </div>
              </div>
            ))}
          {current === tabs[0]?.id && <PreservedList parts={form.preserved} onRemove={removePreserved} />}
        </div>
        {narrow && commands}
      </section>
    </CreatorEnvProvider>
  );
}
