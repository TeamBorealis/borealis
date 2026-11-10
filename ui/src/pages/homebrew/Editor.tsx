import { type ReactNode } from "react";
import type { HomebrewFieldSpec, HomebrewKindDescriptor, HomebrewProblem, HomebrewTab } from "../../api/contract";
import { Icon, TabControl } from "../../components";
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
  const form = draft.form;
  if (!form) return null;
  const name = typeof form.fields.name === "string" && form.fields.name.trim() ? form.fields.name.trim() : `New ${kind.label.toLowerCase()}`;
  const tabs = editorTabs(kind, narrow);
  const current = tabs.some((t) => t.id === tab) ? tab : tabs[0]?.id ?? "";
  const fields = kind.fields.filter((f) => f.tab === current && !f.expert);
  const hasTryIt = kind.tabs.some((t) => t.id === "tryIt");
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
    <section className="hb-editor" aria-label="Element editor">
      <Band title={name}><span className={`hb-autosave hb-${draft.autosave}`} role="status">{AUTOSAVE_TEXT[draft.autosave]}</span></Band>
      {!narrow && commands}
      {draft.error && <p className="hb-error-line" role="alert">{draft.error}</p>}
      <ProblemsPanel problems={draft.problems} checking={draft.checking} onJump={actions.onJump} onOpen={() => void draft.check()} />
      <div className="aur-subband hb-band">
        <TabControl tabs={tabs.map((t) => ({ id: t.id, label: t.label }))} active={current} onChange={onTab} label="Editor sections" idPrefix="hb-tab" />
      </div>
      <div role="tabpanel" id="hb-tab-panel" aria-labelledby={`hb-tab-${current}`} className="hb-tabpanel">
        {current === PREVIEW_TAB.id ? preview : fields.length === 0
          ? <p className="hb-pending">Nothing to fill in here yet.</p>
          : sections(fields).map(([title, list]) => (
            <div key={title || "main"} className="hb-section">
              {title && <h3 className="hb-section-title">{title}</h3>}
              <div className="mg-grid two">
                {list.map((f) => (
                  <FieldView key={f.key} field={f} value={form.fields[f.key]} problems={draft.problems}
                    onChange={(v, typing) => draft.setField(f.key, v, typing)} />
                ))}
              </div>
            </div>
          ))}
      </div>
      {narrow && commands}
    </section>
  );
}
