import { useEffect, useState } from "react";
import type { Api, HomebrewPickerTarget, HomebrewRef } from "../../../api/contract";
import { Icon } from "../../../components";
import { useApi } from "../../../state/ApiContext";
import { ReferencePicker } from "./ReferencePicker";
import { sameId } from "./refRows";

/** Names of elements picked in this session, by lower-case id: an own unsaved element has nothing to look up yet. */
const known = new Map<string, HomebrewRef>();

export function rememberRefs(refs: readonly HomebrewRef[]): void {
  for (const r of refs) if (r.name) known.set(r.id.toLowerCase(), r);
}

/** Looks an id up by its stored spelling, then upper case (ids are upper case; `equipped` stores them lower case). */
async function lookUp(api: Api, id: string): Promise<HomebrewRef> {
  const hit = known.get(id.toLowerCase());
  if (hit) return { ...hit, id };
  for (const tryId of [...new Set([id, id.toUpperCase()])]) {
    try {
      const d = await api.getElement({ elementId: tryId });
      const ref = { id, name: d.name, type: d.type };
      known.set(id.toLowerCase(), ref);
      return ref;
    } catch { /* try the next spelling */ }
  }
  return { id, name: "", type: "", missing: true };
}

/** Bare ids (block fields, builder rows) as references with their names; a missing one has no name. */
export function useRefs(ids: readonly string[]): HomebrewRef[] {
  const api = useApi();
  const key = ids.join("\u0000");
  const [refs, setRefs] = useState<HomebrewRef[]>(() => ids.map((id) => known.get(id.toLowerCase()) ?? { id, name: "", type: "" }));
  useEffect(() => {
    let live = true;
    void Promise.all(ids.map((id) => lookUp(api, id))).then((r) => { if (live) setRefs(r); });
    return () => { live = false; };
  }, [api, key]); // eslint-disable-line react-hooks/exhaustive-deps
  return refs.filter((r) => ids.some((id) => sameId(id, r.id)));
}

/** "Missing: <last known name>" (spec 5.1), never an id. */
export const refText = (r: HomebrewRef): string => (r.missing ? `Missing: ${r.name || "an element that is not loaded"}` : r.name || "...");

export interface RefFieldProps {
  id?: string;
  label: string;
  target: HomebrewPickerTarget;
  value: readonly HomebrewRef[];
  onChange: (refs: HomebrewRef[]) => void;
  describedBy?: string;
  invalid?: boolean;
  disabled?: boolean;
}

/**
 * A reference field (spec 5.1): the chosen element by name, a Pick button that opens the reference picker, and X to clear.
 * A missing element reads "Missing: <name>" in red with Pick again. Several picks (`target.multiple`) list as removable
 * chips. Ids travel and are never shown.
 */
export function RefField({ id, label, target, value, onChange, describedBy, invalid, disabled }: RefFieldProps) {
  const [open, setOpen] = useState(false);
  const multiple = !!target.multiple;
  const missing = value.some((r) => r.missing);
  const choose = (picks: HomebrewRef[]) => {
    // a pick carried into the picker by id keeps the name it had here when its row wasn't loaded there
    const named = picks.map((p) => (p.name ? p : value.find((v) => sameId(v.id, p.id)) ?? p));
    rememberRefs(named);
    setOpen(false);
    onChange(multiple ? named : named.slice(0, 1));
  };
  return (
    <div className={`hb-ref ${invalid ? "hb-invalid" : ""}`} id={id} aria-describedby={describedBy}>
      {value.length > 0 && (
        <ul className="hb-ref-list" aria-label={label}>
          {value.map((r) => (
            <li key={r.id} className={`hb-chip-item ${r.missing ? "hb-missing" : ""}`}>
              {r.missing && <Icon name="alert" />}
              <span className="hb-ref-name">{refText(r)}</span>
              {!disabled && <button type="button" className="hb-chip-x" aria-label={`Remove ${r.name || "the missing element"}`} onClick={() => onChange(value.filter((x) => x.id !== r.id))}><Icon name="x" /></button>}
            </li>
          ))}
        </ul>
      )}
      {!disabled && (
        <button type="button" className="hb-pick-btn" aria-label={`${missing ? "Pick again" : value.length && !multiple ? "Change" : "Pick"}: ${label}`} onClick={() => setOpen(true)}>
          <Icon name="search" />{missing ? "Pick again" : value.length && !multiple ? "Change" : multiple ? "Add" : "Pick"}
        </button>
      )}
      {open && <ReferencePicker title={label} target={target} chosen={value.map((r) => r.id)} onChoose={choose} onClose={() => setOpen(false)} />}
    </div>
  );
}

/** A form value (`HomebrewRef`, a list of them, or null) as a list. */
export function refsOf(value: unknown): HomebrewRef[] {
  const one = (v: unknown): HomebrewRef | null => (typeof v === "object" && v !== null && typeof (v as HomebrewRef).id === "string" ? v as HomebrewRef : null);
  if (Array.isArray(value)) return value.map(one).filter((r): r is HomebrewRef => r !== null);
  const r = one(value);
  return r ? [r] : [];
}

/** A block field holding bare ids (`reference`, `references`): names looked up, changes written back as ids. */
export function IdRefField({ ids, multiple, onIds, ...rest }: Omit<RefFieldProps, "value" | "onChange"> & { ids: readonly string[]; multiple: boolean; onIds: (ids: string[], refs: HomebrewRef[]) => void }) {
  const refs = useRefs(ids);
  const shown = ids.map((id) => refs.find((r) => sameId(r.id, id)) ?? { id, name: "", type: "" });
  return <RefField {...rest} target={{ ...rest.target, multiple }} value={shown} onChange={(next) => onIds(next.map((r) => r.id), next)} />;
}
