import { mdiBackupRestore } from "@mdi/js";
import { IconButton } from "../../components";
import { useCharacter } from "../../state/CharacterContext";
import { PaneShell } from "./DescriptionPane";
import { AbilityGrid } from "./AbilityTile";
import { SlotList } from "./SlotList";
import type { DescribeFn } from "./PickerGroup";

const HELP: { name: string; text: string }[] = [
  { name: "Strength", text: "Raw physical power. It drives melee attacks, carrying capacity and Athletics." },
  { name: "Dexterity", text: "Agility, reflexes and balance. It drives ranged attacks, Armor Class in light armor, initiative, and Acrobatics, Sleight of Hand and Stealth." },
  { name: "Constitution", text: "Health and stamina. It adds to hit points every level and to concentration saves." },
  { name: "Intelligence", text: "Reasoning and memory. It drives Arcana, History, Investigation, Nature and Religion." },
  { name: "Wisdom", text: "Awareness and intuition. It drives Perception, Insight, Medicine, Survival and Animal Handling." },
  { name: "Charisma", text: "Force of personality. It drives Deception, Intimidation, Performance and Persuasion." },
];

export function AbilitiesPane() {
  return (
    <PaneShell title="Ability scores" imageIcon={false} intro="Six scores describe what your character is naturally good at, and where they struggle."
      readText={HELP.map((h) => `${h.name}. ${h.text}`).join(" ")}>
      {HELP.map((h) => <section key={h.name}><h3 className="desc-h">{h.name}</h3><p className="desc-line">{h.text}</p></section>)}
    </PaneShell>
  );
}

export function AbilitiesTab({ describedId, onDescribe }: { describedId: string | null; onDescribe: DescribeFn }) {
  const { snapshot, mutate } = useCharacter();
  if (!snapshot) return null;
  const a = snapshot.abilities;
  const pb = a.pointBuy;
  const random = a.method === "roll_3d6" || a.method === "roll_4d6";
  const slots = snapshot.slots.filter((s) => s.tab === "abilities");
  const reset = () => void mutate((api, id) => api.resetAbilities({ characterId: id }));
  return (
    <div className="abil-page">
      {(pb || random) && (
        <div className="abil-top">
          {pb && snapshot.options.pointBuyUnrestricted && <span className="abil-flag">House rule: unrestricted</span>}
          {pb && (
            <span className={`abil-points ${pb.remaining < 0 ? "over" : ""}`} role="status" aria-label="Point buy budget">
              Ability points: <strong>{pb.budget - pb.remaining} / {pb.budget}</strong>
            </span>
          )}
          {random && (
            <>
              <span className="abil-points">Roll</span>
              <IconButton icon="dice" label="Roll all abilities" onClick={() => void mutate((api, id) => api.rollAbilities({ characterId: id }))} />
              <span className="small muted">{a.rolls.length ? `${a.rolls.length} roll(s)` : "No rolls yet"}</span>
            </>
          )}
          <button type="button" className="abil-restore" aria-label="Reset ability scores" title="Reset ability scores" onClick={reset}>
            <svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true" focusable="false"><path d={mdiBackupRestore} fill="currentColor" /></svg>
          </button>
        </div>
      )}
      {a.method === "standard_array" && a.standardArray && (
        <p className="small muted abil-note" role="status">Unassigned values: {a.standardArray.unassigned.length ? a.standardArray.unassigned.join(", ") : "none"}. Choosing a value that is already used swaps it.</p>
      )}
      <AbilityGrid state={a}
        onSet={(ab, v) => void mutate((api, id) => api.setBaseScore({ characterId: id, ability: ab, value: v }))}
        onRoll={(ab) => void mutate((api, id) => api.rollAbilities({ characterId: id, ability: ab }))}
        onAssign={(ab, v) => void mutate((api, id) => api.assignStandardArray({ characterId: id, ability: ab, value: v }))} />
      {a.overCap.length > 0 && <p className="danger-text abil-note" role="alert"><strong>Some scores exceed the maximum of {a.cap}.</strong> The cap can be changed in Settings.</p>}
      {slots.length > 0 && <SlotList slots={slots} describedId={describedId} onDescribe={onDescribe} />}
    </div>
  );
}
