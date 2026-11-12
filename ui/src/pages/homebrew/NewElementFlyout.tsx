import { type KeyboardEvent, useRef, useState } from "react";
import type { HomebrewKindDescriptor, HomebrewRef } from "../../api/contract";
import { Alert, Button, Expander, Flyout, GuideLink, TextBox } from "../../components";
import { CheckBox } from "../../components/controls";
import { errorMessage } from "../../state/useAsync";
import { ReferencePicker } from "./pickers/ReferencePicker";
import { enterCreates } from "./flyoutKeys";
import "../characters/characters.css";

/** Where a new element starts (spec 11): an empty form, an SRD template (bundled elements only, CS1), or a copy of any
 * loaded element (SRD, imported or own). */
export type StartFrom = "blank" | "template" | "copy";

/** What to copy: the element and, for a class, whether its features and subclasses are copied too. */
export interface CopyChoice {
  copyFrom: string;
  deepCopy: boolean;
}

const STARTS: { id: StartFrom; label: string; description: string }[] = [
  { id: "blank", label: "Blank", description: "An empty form." },
  { id: "template", label: "Start from a template", description: "A copy of an element of the System Reference Document." },
  { id: "copy", label: "New from copy...", description: "A copy of any loaded element: SRD, imported or your own." },
];

/**
 * A radio group of option cards (WAI-ARIA radio group): one Tab stop, the chosen card; the arrow keys (and Home / End)
 * choose the next card and move focus to it (WB-26: each of the 38 kinds was a Tab stop and the arrows did nothing).
 */
function RadioCards({ labelledBy, describedBy, options, value, onChange }: {
  labelledBy: string; describedBy?: string; options: readonly { id: string; label: string; description: string }[]; value: string; onChange: (id: string) => void;
}) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const at = Math.max(0, options.findIndex((o) => o.id === value));
  const go = (i: number) => {
    const next = (i + options.length) % options.length;
    const o = options[next];
    if (!o) return;
    onChange(o.id);
    refs.current[next]?.focus();
  };
  const onKey = (e: KeyboardEvent, i: number) => {
    const to = e.key === "ArrowDown" || e.key === "ArrowRight" ? i + 1 : e.key === "ArrowUp" || e.key === "ArrowLeft" ? i - 1
      : e.key === "Home" ? 0 : e.key === "End" ? options.length - 1 : null;
    if (to === null) return;
    e.preventDefault();
    go(to);
  };
  return (
    <div className="v4-opts hb-kind-cards" role="radiogroup" aria-labelledby={labelledBy} aria-describedby={describedBy}>
      {options.map((o, i) => (
        <button key={o.id} ref={(el) => { refs.current[i] = el; }} type="button" role="radio" aria-checked={o.id === value} tabIndex={i === at ? 0 : -1}
          className="v4-opt hb-kind-card" onClick={() => onChange(o.id)} onKeyDown={(e) => onKey(e, i)}>
          <span className="hb-kind-name">{o.label}</span>
          <span className="hb-kind-desc">{o.description}</span>
        </button>
      ))}
    </div>
  );
}

/** Kinds offered by New element, in the CATEGORIES order: main forms, small types, then the expert form. Child kinds are
 * made inside their parent (a class's features). */
export function creatableKinds(kinds: readonly HomebrewKindDescriptor[]): HomebrewKindDescriptor[] {
  const order: Record<string, number> = { main: 0, small: 1, expert: 2 };
  return kinds.filter((k) => k.group !== "child").sort((a, b) => (order[a.group] ?? 3) - (order[b.group] ?? 3));
}

/**
 * New element (spec 13.11), in the New Character flyout's look: one dashed option card per kind with its one-line
 * explanation, the name field, CREATE / CANCEL.
 */
export function NewElementFlyout({ kinds, initialKind, onCreate, onClose }: {
  kinds: readonly HomebrewKindDescriptor[]; initialKind: string | null;
  onCreate: (kindId: string, name: string, copy?: CopyChoice) => Promise<void>; onClose: () => void;
}) {
  const offered = creatableKinds(kinds);
  const [kindId, setKindId] = useState(initialKind ?? offered[0]?.id ?? "");
  const [name, setName] = useState("");
  const [start, setStart] = useState<StartFrom>("blank");
  const [deep, setDeep] = useState(true);
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const kind = offered.find((k) => k.id === kindId) ?? null;
  const run = async (copy?: CopyChoice) => {
    if (!kindId) return;
    setBusy(true);
    try { await onCreate(kindId, name.trim(), copy); } catch (e) { setError(errorMessage(e)); } finally { setBusy(false); }
  };
  const create = () => {
    if (start === "blank") void run();
    else setPicking(true);
  };
  const picked = (picks: HomebrewRef[]) => {
    setPicking(false);
    const id = picks[0]?.id;
    if (id) void run({ copyFrom: id, deepCopy: deep });
  };
  return (
    <Flyout title="New element" onClose={onClose} headAction={<GuideLink guide="homebrew-creator" />}
      footer={<div className="v4-foot"><Button data-testid="create-element" disabled={busy || !kindId} onClick={create}>{start === "blank" ? "Create" : "Choose..."}</Button><Button onClick={onClose}>Cancel</Button></div>}>
      <form className="v4-new" onSubmit={(e) => { e.preventDefault(); void create(); }} onKeyDown={enterCreates(() => { if (!busy && kindId) create(); })}>
        <Expander flyout title="Element">
          <p className="sub">Choose what to make, then give it a name. You can rename it at any time.</p>
          <label className="v4-fl" htmlFor="hb-new-name">Name</label>
          <TextBox light id="hb-new-name" value={name} placeholder="Unnamed" data-autofocus onChange={(e) => setName(e.target.value)} />
          {error && <Alert tone="danger" role="alert">{error}</Alert>}
        </Expander>
        <section className="aur-expander flyout-variant v4-plain" aria-labelledby="hb-kinds-title">
          <div className="aur-expander-head"><h2 className="aur-expander-toggle" id="hb-kinds-title"><span className="aur-expander-title">Kind</span></h2></div>
          <div className="aur-expander-body">
            {/* HC10 Q5: a description line under KIND and START FROM, as under the creator's fields */}
            <p className="hb-field-help" id="hb-kinds-help">What to make. Features and traits go inside their element.</p>
            <RadioCards labelledBy="hb-kinds-title" describedBy="hb-kinds-help" options={offered} value={kindId} onChange={setKindId} />
          </div>
        </section>
        <section className="aur-expander flyout-variant v4-plain" aria-labelledby="hb-start-title">
          <div className="aur-expander-head"><h2 className="aur-expander-toggle" id="hb-start-title"><span className="aur-expander-title">Start from</span></h2></div>
          <div className="aur-expander-body">
            <p className="hb-field-help" id="hb-start-help">Blank is empty; a copy fills the form, the original stays.</p>
            <RadioCards labelledBy="hb-start-title" describedBy="hb-start-help" options={STARTS} value={start} onChange={(id) => setStart(id as StartFrom)} />
            {start !== "blank" && kind?.id === "class" && (
              <label className="hb-inline-check"><CheckBox checked={deep} onChange={(e) => setDeep(e.target.checked)} />Copy its features and subclasses too</label>
            )}
          </div>
        </section>
      </form>
      {picking && kind && (
        <ReferencePicker title={start === "template" ? `${kind.label} template` : `${kind.label} to copy`} target={{ types: kind.elementTypes }} chosen={[]}
          filters={{ srdOnly: start === "template" }} onChoose={picked} onClose={() => setPicking(false)} />
      )}
    </Flyout>
  );
}
