import { useState } from "react";
import type { CompanionFeature } from "../../api/contract";
import { Button, Modal, TextArea, TextField } from "../../components";
import { MgSection } from "./Field";
import { MgIcon } from "./mgIcons";

/** Mirrors the backend limits (`borealis_character::companions`). */
const MAX_NAME = 60;
const MAX_TEXT = 2000;
const MAX_TRAITS = 30;

interface Editing { at: number | null; trait: CompanionFeature }

function TraitDialog({ editing, onSave, onClose }: { editing: Editing; onSave: (t: CompanionFeature) => void; onClose: () => void }) {
  const [t, setT] = useState(editing.trait);
  const ok = t.name.trim().length > 0;
  return (
    <Modal title={editing.at === null ? "New trait" : `Edit ${editing.trait.name}`} onClose={onClose}
      footer={<><Button onClick={onClose}>Cancel</Button><Button variant="primary" disabled={!ok} onClick={() => onSave(t)}>Save</Button></>}>
      <div className="stack">
        <TextField label="Name" value={t.name} maxLength={MAX_NAME} data-autofocus onChange={(e) => setT({ ...t, name: e.target.value })} />
        <TextArea label="Text" value={t.text} maxLength={MAX_TEXT} onChange={(e) => setT({ ...t, text: e.target.value })} />
      </div>
    </Modal>
  );
}

/**
 * The selected companion's own traits (GitHub #13): Add Trait, then one card per trait with Edit and Remove, in the Attacks tab's
 * card style. The list is saved whole on every change.
 */
export function CompanionTraits({ traits, onChange }: { traits: CompanionFeature[]; onChange: (next: CompanionFeature[]) => void }) {
  const [editing, setEditing] = useState<Editing | null>(null);
  const save = (t: CompanionFeature) => {
    const at = editing?.at ?? null;
    setEditing(null);
    onChange(at === null ? [...traits, t] : traits.map((old, i) => (i === at ? t : old)));
  };
  return (
    <MgSection title="Traits" sub="Your own traits for this companion: training, upgrades or anything that changed it. They follow the creature's traits in the companion view.">
      <div className="mg-toolbar" role="toolbar" aria-label="Trait actions">
        <button type="button" className="mg-tool wide cmp-tool" disabled={traits.length >= MAX_TRAITS} onClick={() => setEditing({ at: null, trait: { name: "", text: "" } })}>
          <MgIcon name="plus" />Add Trait
        </button>
      </div>
      {traits.length > 0 && (
        <ul className="atk-list" aria-label="Traits">
          {traits.map((t, i) => (
            <li key={`${i}-${t.name}`} className="atk cmp-trait">
              <div className="info"><h3>{t.name}</h3>{t.text && <p>{t.text}</p>}</div>
              <span className="btns">
                <button type="button" aria-label={`Edit ${t.name}`} title={`Edit ${t.name}`} onClick={() => setEditing({ at: i, trait: t })}><MgIcon name="edit" /></button>
                <button type="button" aria-label={`Remove ${t.name}`} title={`Remove ${t.name}`} onClick={() => onChange(traits.filter((_, j) => j !== i))}><MgIcon name="cardMinus" /></button>
              </span>
            </li>
          ))}
        </ul>
      )}
      {editing && <TraitDialog editing={editing} onSave={save} onClose={() => setEditing(null)} />}
    </MgSection>
  );
}
