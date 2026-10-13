import { useEffect, useState } from "react";
import type { CharacterDetails } from "../../api/contract";
import { ComboBox, TextBox } from "../../components";
import { useCharacter } from "../../state/CharacterContext";
import { MgField } from "./Field";
import { MgIcon } from "./mgIcons";

/** Field bound to a CharacterDetails key; commits on blur. `area` = multi-line, `choices` = fixed dropdown, `list` = free text with suggestions. */
export function DetailField({ k, label, area, list, choices, className, tall, hint, ok }: { k: keyof CharacterDetails; label: string; area?: boolean; list?: string[]; choices?: string[]; className?: string; tall?: boolean; hint?: string; ok?: boolean }) {
  const { snapshot, mutate } = useCharacter();
  const current = snapshot?.details[k] ?? "";
  const [v, setV] = useState(current);
  useEffect(() => setV(current), [current]);
  const commitValue = (next: string) => { if (next !== current) void mutate((api, id) => api.updateDetails({ characterId: id, patch: { [k]: next } })); };
  const listId = list ? `dl-${String(k)}` : undefined;
  return (
    <MgField label={<>{label}{ok && v ? <span className="mg-ok" role="img" aria-label="set"><MgIcon name="check" /></span> : null}</>} className={className}>
      {(id) => {
        if (area) return <><textarea id={id} className={`textarea ${tall ? "tall" : ""}`} value={v} onChange={(e) => setV(e.target.value)} onBlur={() => commitValue(v)} />{hint && <span className="mg-hint">{hint}</span>}</>;
        if (choices) {
          const opts = ["", ...choices, ...(v && !choices.includes(v) ? [v] : [])];
          return <ComboBox id={id} value={v} onChange={(e) => { setV(e.target.value); commitValue(e.target.value); }} options={opts.map((o) => ({ value: o, label: o }))} />;
        }
        return (
          <>
            {hint && <span className="mg-hint">{hint}</span>}
            <TextBox id={id} value={v} list={listId} onChange={(e) => setV(e.target.value)} onBlur={() => commitValue(v)} />
            {list && <datalist id={listId}>{list.map((o) => <option key={o} value={o} />)}</datalist>}
          </>
        );
      }}
    </MgField>
  );
}
