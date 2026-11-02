import { mdiImageSearch } from "@mdi/js";
import { useState } from "react";
import type { CompanionPatch, CompanionState } from "../../api/contract";
import { NumericUpDown, TextBox, useConfirm } from "../../components";
import { useCharacter } from "../../state/CharacterContext";
import { CompanionTraits } from "./CompanionTraits";
import { CreaturePicker } from "./CreaturePicker";
import { MgField, MgSection } from "./Field";
import { MgIcon } from "./mgIcons";
import { PortraitsFlyout } from "./PortraitsFlyout";
import "./companions.css";

const SUB_HEADER = "A companion that accompanies you on your adventures and is trained to fight alongside you.";
const MAX_BONUS = 99;
const BONUSES = [["armorClassBonus", "Armor Class Bonus", 1], ["hitPointsBonus", "Hit Points Bonus", 1], ["speedBonus", "Speed Bonus", 5]] as const;

const creatureLine = (c: CompanionState): string => (c.creature ? [c.creature.name, c.build].filter(Boolean).join(" · ") : "No creature");

/** The switcher (GitHub #13): one card per companion in the Attacks tab's card style; added ones can be removed. */
function CompanionList({ list, selected, onSelect, onRemove }: {
  list: CompanionState[]; selected: string; onSelect: (id: string) => void; onRemove: (c: CompanionState) => void;
}) {
  return (
    <ul className="atk-list cmp-list" aria-label="Companions">
      {list.map((c) => (
        <li key={c.id} className="atk" aria-current={c.id === selected} onClick={() => onSelect(c.id)}>
          <div className="info">
            <h3>{c.name || "Companion"}</h3>
            <p>{creatureLine(c)}</p>
            {c.granted && <span className="ap-tags"><span className="ap-tag">From a feature</span></span>}
          </div>
          {!c.granted && (
            <span className="btns">
              <button type="button" aria-label={`Remove ${c.name || "companion"}`} title={`Remove ${c.name || "companion"}`} onClick={(e) => { e.stopPropagation(); onRemove(c); }}>
                <MgIcon name="cardMinus" />
              </button>
            </span>
          )}
        </li>
      ))}
    </ul>
  );
}

/** Notes, saved when the field loses focus. */
function NotesField({ value, onCommit }: { value: string; onCommit: (v: string) => void }) {
  const [v, setV] = useState(value);
  return (
    <MgField label="Notes">
      {(id) => <textarea id={id} className="textarea" value={v} maxLength={4000} onChange={(e) => setV(e.target.value)} onBlur={() => { if (v !== value) onCommit(v); }} />}
    </MgField>
  );
}

/**
 * The classic builder's COMPANION tab: header, one sentence, the Companion Name field (the creature's name until the user types another;
 * clearing it restores that name) and the Companions Gallery button. The statistics live in the information slider's companion context.
 * GitHub #13 adds Add Companion, a switcher when there is more than one, the creature of an added companion, and each companion's own
 * traits, bonuses and notes.
 */
export function CompanionTab() {
  const { snapshot, mutate } = useCharacter();
  const confirm = useConfirm();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [galleryOpen, setGalleryOpen] = useState(false);
  const [picker, setPicker] = useState<"add" | "change" | null>(null);
  const list = snapshot?.companions ?? [];
  const c = list.find((x) => x.id === selectedId) ?? list[0];
  if (!snapshot) return null;
  const add = async (creatureId: string | null) => {
    setPicker(null);
    const s = await mutate((api, cid) => api.addCompanion({ characterId: cid, creatureId }));
    if (s) setSelectedId(s.companions.at(-1)?.id ?? null);
  };
  const addButton = <button type="button" className="mg-tool wide cmp-tool" onClick={() => setPicker("add")}><MgIcon name="plus" />Add Companion</button>;
  const addPicker = picker === "add" && (
    <CreaturePicker title="Add Companion" ruleset={snapshot.ruleset} onClose={() => setPicker(null)} onPick={(id) => void add(id)} />
  );
  // U18: the tab shows for every character (Aurora hides it without a companion), so anyone can add a pet, mount or hireling.
  if (!c) return (
    <div className="mg-page">
      <MgSection title="COMPANION" sub={SUB_HEADER}>
        <div className="mg-toolbar" role="toolbar" aria-label="Companion actions">{addButton}</div>
        <p className="cmp-empty">No companion yet. Add one to keep a familiar, beast companion, mount or pet with its own name, picture, traits and notes.</p>
      </MgSection>
      {addPicker}
    </div>
  );
  const update = (patch: CompanionPatch) => void mutate((api, cid) => api.updateCompanion({ characterId: cid, companionId: c.id, patch }));
  const original = c.creature?.name ?? "";
  const rename = (typed: string) => update({ name: typed.trim() === original ? "" : typed.trim() });
  const remove = async (r: CompanionState) => {
    const answer = await confirm({ title: `Remove ${r.name || "this companion"}?`, body: "Its name, picture, traits, bonuses and notes are deleted with it.", confirmLabel: "Remove", danger: true });
    if (answer === "confirm") void mutate((api, cid) => api.removeCompanion({ characterId: cid, companionId: r.id }));
  };
  return (
    <div className="mg-page">
      <MgSection title="COMPANION" sub={SUB_HEADER}>
        <div className="mg-toolbar" role="toolbar" aria-label="Companion actions">{addButton}</div>
        {list.length > 1 && <CompanionList list={list} selected={c.id} onSelect={setSelectedId} onRemove={(r) => void remove(r)} />}
        <div className="mg-grid cmp-fields">
          <MgField label="Companion Name">
            {(id) => <TextBox key={`${c.id}:${c.name}`} id={id} defaultValue={c.name} maxLength={c.granted ? undefined : 60} onBlur={(e) => rename(e.target.value)} />}
          </MgField>
        </div>
        <p className="cmp-creature">
          <span className="cmp-label">Creature:</span> <b>{c.creature?.name ?? "None"}</b>
          {c.granted ? <span className="cmp-hint">Chosen on the builder's Companion tab.</span> : (
            <button type="button" className="mg-tool wide cmp-tool" onClick={() => setPicker("change")}>Choose Creature</button>
          )}
        </p>
        <div style={{ marginTop: 10 }}>
          <button type="button" className="mg-tool wide mg-tool-gallery" onClick={() => setGalleryOpen(true)}>
            <svg className="icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d={mdiImageSearch} /></svg>Companions Gallery
          </button>
        </div>
      </MgSection>
      <CompanionTraits key={c.id} traits={c.customTraits} onChange={(traits) => update({ traits })} />
      <MgSection title="Adjustments" sub="Bonuses added to the creature's armor class, hit points and walking speed.">
        <div className="mg-grid cmp-bonuses">
          {BONUSES.map(([key, label, step]) => (
            <div key={key} className="mg-f"><span className="mg-label" aria-hidden="true">{label}</span>
              <NumericUpDown label={label} value={c[key]} min={-MAX_BONUS} max={MAX_BONUS} step={step} onChange={(v) => update({ [key]: v })} />
            </div>
          ))}
        </div>
      </MgSection>
      <MgSection title="Companion Notes" sub="Anything else about this companion.">
        <NotesField key={c.id} value={c.notes} onCommit={(notes) => update({ notes })} />
      </MgSection>
      {galleryOpen && <PortraitsFlyout current={c.portrait} onClose={() => setGalleryOpen(false)} onSelect={(portrait) => update({ portrait })} />}
      {addPicker}
      {picker === "change" && (
        <CreaturePicker title="Choose Creature" ruleset={snapshot.ruleset} onClose={() => setPicker(null)}
          onPick={(id) => { setPicker(null); update({ creatureId: id }); }} />
      )}
    </div>
  );
}
