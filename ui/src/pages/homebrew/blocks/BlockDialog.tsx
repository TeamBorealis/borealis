import { useId, useState } from "react";
import type { HomebrewBlock, HomebrewBlockSpec, HomebrewBuilderProblem, HomebrewCondition, HomebrewPreservedPart } from "../../../api/contract";
import { Button, Dialog, Icon, NumericUpDown, ToggleSwitch, useConfirm } from "../../../components";
import { ConditionBuilder } from "../builders/ConditionBuilder";
import { asExprValue } from "../builders/words";
import { useCreatorEnv, WithExpert } from "../env";
import { FieldList } from "./BlockFieldControl";
import { NumbersControl } from "./valueControls";
import { type MadeOption, OPTION_TYPE, OptionGroupPanel } from "./OptionGroupPanel";
import { useApi } from "../../../state/ApiContext";
import { noteStatus } from "../statusNote";
import { errorMessage } from "../../../state/useAsync";
import { fallbackSentence } from "./sentence";
import { type Fields, cleanFields, isEmptyValue, missingRequired, withAutoType } from "./values";

/** Levels a "From level" gate offers (class and character levels). */
const MAX_LEVEL = 20;

export interface BlockDialogProps {
  spec: HomebrewBlockSpec;
  block: HomebrewBlock;
  /** A block just picked from the Add menu (OK adds it; Cancel adds nothing). */
  isNew: boolean;
  /** The draft's problems for this block, relative to its fields (`number`, `requirements.items.0`). */
  problems: readonly HomebrewBuilderProblem[];
  onOk: (block: HomebrewBlock) => void;
  onCancel: () => void;
  /** "Also gained again at levels" (a class feature's block, spec 4.5): the levels its copies are written at. */
  repeat?: boolean;
}

/** "A", "A and B", "A, B and C". */
export function namesText(names: readonly string[]): string {
  return names.length < 2 ? names.join("") : `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
}

/** "Kept from the original" (spec 3.4): read-only parts in plain words, each with Remove. Nothing can be added. */
export function PreservedList({ parts, onRemove, label = "Kept from the original" }: { parts: readonly HomebrewPreservedPart[]; onRemove: (key: string) => void; label?: string }) {
  if (parts.length === 0) return null;
  return (
    <section className="hb-preserved" aria-label={label}>
      <h4 className="hb-section-title">{label}</h4>
      <p className="hb-field-help">Borealis keeps these parts unchanged. They can be removed, not edited.</p>
      <ul className="hb-ref-list">
        {parts.map((p) => (
          <li key={p.key} className="hb-chip-item hb-kept-item">
            <span className="hb-ref-name">{p.label}</span>
            <button type="button" className="hb-chip-x" aria-label={`Remove ${p.label}`} onClick={() => onRemove(p.key)}><Icon name="x" /></button>
          </li>
        ))}
      </ul>
    </section>
  );
}

/**
 * The generic block dialog (spec 6.1-6.4, 13.6): rendered from the block's catalog spec, sized and laid out like the
 * Attack editor (fields, then OK / Cancel). "From level" for gated kinds, "Only if" (the condition builder) where the
 * engine reads requirements, an Expert toggle for rarely used fields, and the parts kept from the original with Remove.
 * Swiping the phone sheet down (or Escape) cancels; it asks first only when something was changed (spec 16, O4 B).
 */
export function BlockDialog({ spec, block, isNew, problems, onOk, onCancel, repeat = false }: BlockDialogProps) {
  const env = useCreatorEnv();
  const api = useApi();
  const confirm = useConfirm();
  const levelId = useId();
  // class options made in this dialog (saved at once): Cancel deletes them again (P3)
  const [made, setMade] = useState<readonly MadeOption[]>([]);
  const [fields, setFields] = useState<Fields>(block.fields);
  const [preserved, setPreserved] = useState(block.preserved ?? []);
  const [expert, setExpert] = useState(env.expert);
  const [tried, setTried] = useState(false);
  const dirty = JSON.stringify(fields) !== JSON.stringify(block.fields) || preserved.length !== (block.preserved ?? []).length || made.length > 0;
  const missing = missingRequired(spec.fields, fields);
  const local: HomebrewBuilderProblem[] = tried ? missing.map((f) => ({ path: f.key, message: `Fill in ${f.label.toLowerCase()}.` })) : [];
  const all = [...local, ...problems];
  const hasExpert = spec.fields.some((f) => f.expert);

  // A grant's element type is the creator's to write (the backend does not compute it, hc-wave2-join).
  const setValues = (next: Fields) => setFields((before) => withAutoType(spec.fields, before, next));
  const ok = () => {
    if (missing.length > 0) { setTried(true); return; }
    const clean = cleanFields(fields);
    const next: HomebrewBlock = { ...block, fields: clean, ...(block.preserved ? { preserved } : {}) };
    onOk({ ...next, sentence: fallbackSentence(spec, next, env) });
  };
  const cancel = async () => {
    const names = made.map((o) => o.name);
    const body = names.length > 0
      ? `Discard the changes made to this rule? The ${names.length === 1 ? "option" : "options"} made here (${namesText(names)}) ${names.length === 1 ? "is" : "are"} deleted from the pack too.`
      : "Discard the changes made to this rule?";
    if (dirty && (await confirm({ title: "Discard changes?", body, confirmLabel: "Discard", danger: true })) !== "confirm") return;
    if (made.length > 0 && env.packId) {
      try {
        await api.homebrewDeleteElements({ packId: env.packId, elementIds: made.map((o) => o.id), confirm: true });
        env.onPackChanged?.();
        noteStatus(`Deleted ${namesText(names)} from the pack.`);
      } catch (e) {
        noteStatus(errorMessage(e));
      }
    }
    onCancel();
  };
  const level = typeof fields.level === "number" ? fields.level : 1;
  return (
    <Dialog title={isNew ? `New rule: ${spec.label}` : spec.label} onClose={() => void cancel()} className="hb-block-dialog"
      headAction={hasExpert ? <span className="hb-expert"><ToggleSwitch label="Expert" checked={expert} onChange={setExpert} onLabel="Expert" offLabel="Expert" /></span> : undefined}
      footer={<><Button onClick={() => void cancel()}>Cancel</Button><Button variant="primary" onClick={ok}>OK</Button></>}>
      <WithExpert expert={expert}>
        <div className="hb-block-body">
          {spec.levelGated && (
            <div className="mg-f hb-field">
              <label htmlFor={levelId}>From level</label>
              <span id={levelId}><NumericUpDown label="From level" value={level} min={1} max={MAX_LEVEL} onChange={(n) => setFields((f) => {
                const { level: _l, ...rest } = f;
                return n === 1 && block.fields.level === undefined ? rest : { ...rest, level: n };
              })} /></span>
            </div>
          )}
          {repeat && spec.levelGated && (
            <div className="mg-f hb-field hb-bf-wide" data-field="againAt">
              <label>Also gained again at levels</label>
              <NumbersControl label="Also gained again at levels" min={1} max={MAX_LEVEL}
                value={(Array.isArray(fields.againAt) ? fields.againAt as number[] : []).filter((n) => n !== level)}
                onChange={(v) => setFields((f) => { const { againAt: _a, ...rest } = f; return v && v.length > 0 ? { ...rest, againAt: v } : rest; })} />
              <p className="hb-field-help">The rule is written again at each of these levels (a choice at level 4, 8 and 12; a counter that grows).</p>
            </div>
          )}
          {spec.fields.length === 0 && <p className="hb-field-help">This rule has nothing to fill in.</p>}
          <FieldList fields={spec.fields} values={fields} onValues={setValues} path="" problems={all} />
          {/* a choice of class options makes its group and options in one flow (spec 4.8) */}
          {spec.kind === "select" && fields.type === OPTION_TYPE && <OptionGroupPanel fields={fields} onFields={setFields} onMade={(o) => setMade((m) => [...m, o])} />}
          {spec.onlyIf && (
            <div className="mg-f hb-field hb-bf-wide" data-field="requirements">
              <label>Only if</label>
              <ConditionBuilder label="Only if" value={asExprValue<HomebrewCondition>(fields.requirements)}
                onChange={(v) => setFields((f) => { const { requirements: _r, ...rest } = f; return isEmptyValue(v) && f.requirements === undefined ? rest : { ...rest, requirements: v }; })}
                problems={all.filter((p) => p.path.startsWith("requirements.")).map((p) => ({ ...p, path: p.path.slice("requirements.".length) }))} />
            </div>
          )}
          <PreservedList parts={preserved} onRemove={(key) => setPreserved((p) => p.filter((x) => x.key !== key))} />
          {tried && missing.length > 0 && <p className="hb-error-line" role="alert">Fill in the parts marked with a star before OK.</p>}
        </div>
      </WithExpert>
    </Dialog>
  );
}
