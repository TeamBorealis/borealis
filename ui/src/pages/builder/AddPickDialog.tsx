import { useMemo, useState } from "react";
import type { AddedKind, AddPickOption } from "../../api/contract";
import { Alert, Button, Modal, PrereqUnmetMark, SearchInput } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useAsync } from "../../state/useAsync";
import { setStatus } from "../../state/status";
import { haptic } from "../../lib/haptics";

/** The Add action's singular noun per kind ("Add Feat", "No feat matches."). */
export const KIND_NOUN: Record<AddedKind, string> = { feat: "Feat", spell: "Spell", language: "Language", proficiency: "Proficiency" };

const levelText = (level: number): string => (level === 0 ? "Cantrip" : `Level ${level}`);

/** The second line of a row: the proficiency group or spell level, then the prerequisite, else the source. */
function detailOf(o: AddPickOption): string {
  const head = o.group ?? (o.level !== null ? levelText(o.level) : null);
  const tail = o.prerequisite ? `Prerequisite: ${o.prerequisite}` : o.sourceName;
  return [head, tail].filter(Boolean).join(" · ");
}

const matches = (o: AddPickOption, words: string[]): boolean => {
  const hay = `${o.name} ${o.group ?? ""} ${o.sourceName}`.toLowerCase();
  return words.every((w) => hay.includes(w));
};

export interface AddPickDialogProps {
  kind: AddedKind;
  /** A spell's spellcasting class (its name). */
  spellcasting?: string;
  onClose: () => void;
  /** Called with the added element id after the pick is added. */
  onAdded?: (elementId: string) => void;
}

/**
 * B3 (GitHub #35): picks an extra the DM grants. Lists what the host offers: options the character qualifies for, or,
 * under "Ignore feat prerequisites", every installed one (the others marked "prerequisites not met"). Choosing a row adds
 * it and closes the dialog. A bottom sheet on phones (Dialog).
 */
export function AddPickDialog({ kind, spellcasting, onClose, onAdded }: AddPickDialogProps) {
  const api = useApi();
  const { snapshot, mutate, busy } = useCharacter();
  const characterId = snapshot?.id ?? "";
  const [query, setQuery] = useState("");
  const options = useAsync(
    () => api.listAddedPickOptions({ characterId, kind, ...(spellcasting ? { spellcasting } : {}) }),
    [api, characterId, kind, spellcasting],
  );
  const noun = KIND_NOUN[kind];
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const rows = useMemo(() => (options.data ?? []).filter((o) => matches(o, words)), [options.data, words.join(" ")]); // eslint-disable-line react-hooks/exhaustive-deps
  const title = spellcasting ? `Add ${spellcasting} Spell` : `Add ${noun}`;
  const add = async (o: AddPickOption) => {
    haptic("choose");
    const next = await mutate((a, id) => a.addPick({ characterId: id, kind, elementId: o.id, ...(spellcasting ? { spellcasting } : {}) }));
    if (!next) return;
    setStatus(`${o.name} was added.`);
    onAdded?.(o.id);
    onClose();
  };
  return (
    <Modal title={title} onClose={onClose} className="addpick" footer={<Button onClick={onClose}>Cancel</Button>}>
      <div className="stack addpick-body">
        <p className="addpick-hint">Add a {noun.toLowerCase()} your DM grants. It is marked as added and can be removed again.</p>
        <SearchInput label={`Search ${noun.toLowerCase()} options`} value={query} onChange={setQuery} data-autofocus />
        {options.error && <Alert tone="danger">{options.error}</Alert>}
        {options.data && rows.length === 0 && (
          <p className="addpick-empty">{options.data.length === 0 ? `No ${noun.toLowerCase()} can be added.` : `No ${noun.toLowerCase()} matches.`}</p>
        )}
        <ul className="addpick-list" aria-label={`${noun} options`}>
          {rows.map((o) => (
            <li key={o.id}>
              <button type="button" disabled={busy} onClick={() => void add(o)}>
                <span className="addpick-name"><strong>{o.name}</strong>{!o.qualifies && <PrereqUnmetMark />}</span>
                <span className="addpick-sub">{detailOf(o)}</span>
              </button>
            </li>
          ))}
        </ul>
      </div>
    </Modal>
  );
}
