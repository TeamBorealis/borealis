import { useState } from "react";
import type { AddedKind, AddedPick } from "../../api/contract";
import { AddedMark, Button, Icon, PrereqUnmetMark } from "../../components";
import { haptic } from "../../lib/haptics";
import { useCharacter } from "../../state/CharacterContext";
import { setStatus } from "../../state/status";
import { AddPickDialog, KIND_NOUN } from "./AddPickDialog";
import "./builder.css";

const PLURAL: Record<AddedKind, string> = { feat: "feats", spell: "spells", language: "languages", proficiency: "proficiencies" };

/** The notice of a pick kept but not applied (its prerequisites fail while the house rule is off), as HR3's slot notice. */
export const withheldText = (name: string): string =>
  `${name} does not meet its prerequisites. It is kept but not applied while the "Ignore feat prerequisites" house rule is off.`;

/** The added picks of `kind` (a spell: of class `spellcasting`), in the order they were added. */
export function picksOf(picks: AddedPick[], kind: AddedKind, spellcasting?: string): AddedPick[] {
  return picks.filter((p) => p.kind === kind && (kind !== "spell" || p.spellcasting === spellcasting));
}

export interface AddedPicksProps {
  kind: AddedKind;
  /** A spell section's spellcasting class (its name). */
  spellcasting?: string;
  /** Shows a pick in the page's description pane. */
  onDescribe?: (elementId: string) => void;
}

/**
 * B3 (GitHub #35): a builder section's extras the DM grants: "Add <kind>" and the picks added so far, each a chosen-pick
 * line (as a picker's footer) with the "Added" tag and a remove button. Aurora keeps such extras as "Additional ..."
 * items in the equipment list; Borealis lists them where the kind is built instead (they stay out of the inventory).
 */
export function AddedPicks({ kind, spellcasting, onDescribe }: AddedPicksProps) {
  const { snapshot, mutate, busy } = useCharacter();
  const [adding, setAdding] = useState(false);
  if (!snapshot) return null;
  const picks = picksOf(snapshot.addedPicks ?? [], kind, spellcasting);
  const noun = KIND_NOUN[kind];
  const addLabel = spellcasting ? `Add ${spellcasting} Spell` : `Add ${noun}`;
  const remove = async (p: AddedPick) => {
    haptic("delete");
    const next = await mutate((api, id) => api.removePick({ characterId: id, inventoryId: p.inventoryId }));
    if (next) setStatus(`${p.name} was removed.`);
  };
  return (
    <section className="added-picks" aria-label={`Added ${PLURAL[kind]}`}>
      <div className="added-head">
        <h3 className="pick-group-label">Added {PLURAL[kind]}</h3>
        <Button small icon="plus" className="added-add" disabled={busy} onClick={() => setAdding(true)}>{addLabel}</Button>
      </div>
      {picks.length === 0
        ? <p className="added-empty">Extras your DM grants. Nothing added yet.</p>
        : (
          <ul className="added-list" aria-label={`Added ${PLURAL[kind]} list`}>
            {picks.map((p) => (
              <li key={p.inventoryId}>
                <div className="pick-chosen added-line">
                  <Icon name="check-box" />
                  <button type="button" className="pick-chosen-name added-name" onClick={() => onDescribe?.(p.elementId)}>{p.name}</button>
                  <AddedMark />
                  {p.prerequisitesUnmet && <PrereqUnmetMark />}
                  <span className="pick-chosen-src added-src">{p.sourceName}</span>
                  <button type="button" className="pick-x" aria-label={`Remove ${p.name}`} title={`Remove ${p.name}`} disabled={busy} onClick={() => void remove(p)}><Icon name="x" /></button>
                </div>
                {!p.applied && <p className="pick-empty added-withheld" role="note">{withheldText(p.name)}</p>}
              </li>
            ))}
          </ul>
        )}
      {adding && <AddPickDialog kind={kind} spellcasting={spellcasting} onClose={() => setAdding(false)} onAdded={(id) => onDescribe?.(id)} />}
    </section>
  );
}
