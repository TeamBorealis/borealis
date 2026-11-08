import { useState } from "react";
import { TextBox } from "../../components";
import { randomName } from "../../lib/format";
import { useCharacter } from "../../state/CharacterContext";
import { DetailField } from "./DetailField";
import { MgField, MgSection } from "./Field";
import { MgIcon } from "./mgIcons";
import { XpField } from "./XpField";
import "./manage-fx15.css";

/* B-191: Alignment and Deity are the character's Alignment and Deity selections, as the classic builder's pickers are: they list the
   elements those slots offer (snapshot.detailChoices), and a value naming one selects it. Alignment is a closed list (a rules value that
   attunement and prerequisites test); a deity may also be typed (issue #30), kept as custom text. These lists stand in only when the
   content offers no such elements. */
const ALIGNMENTS = ["Lawful Good", "Neutral Good", "Chaotic Good", "Lawful Neutral", "Neutral", "Chaotic Neutral", "Lawful Evil", "Neutral Evil", "Chaotic Evil"];
/* Editable combos (issue #30): suggestions, any typed text is kept. Gender, eyes, skin and hair are the classic builder's lists;
   Gender also offers Non-binary before Construct, as the New Character dialog does (decision FU1). */
const GENDERS = ["Male", "Female", "Non-binary", "Construct"];
const EYES = ["Blue", "Brown", "Red", "Green", "Black", "White", "Purple", "Orange", "Pink", "None", "Eye Patch", "Glass Eye"];
const SKIN = ["Pale", "Fair", "White", "Brown", "Black", "Albino", "Obsidian", "Red Scales", "Brass Scales", "Green Scales", "Black Scales", "Furry", "Barkskin"];
const HAIR = ["Bald", "Short", "Long", "Brown", "Red", "White", "Gray", "Black", "Purple", "Green"];
const DEITIES = ["Apollo", "Athena", "Brigid", "Demeter", "Freya", "Hades", "Hermes", "Isis", "Odin", "Osiris", "Ra", "Thor", "Tyr", "Zeus"];

const CHARACTER_INTRO = "A hero is far more than a species and a class. Here you record who they are: their name and looks, where they came from, the languages they speak, the things they believe in, and the temperament and ideals that set them apart from every other adventurer you will ever meet along the road.";

export function CharacterTab() {
  const { snapshot, mutate } = useCharacter();
  const [name, setName] = useState<string | null>(null);
  if (!snapshot) return null;
  const choices = snapshot.detailChoices;
  const alignments = choices.alignment.length ? choices.alignment : ALIGNMENTS;
  const deities = choices.deity.length ? choices.deity : DEITIES;
  const rename = (next: string) => void mutate((api, cid) => api.updateCharacterMeta({ characterId: cid, name: next }));
  return (
    <div className="mg-page">
      <MgSection title="Character details" sub={CHARACTER_INTRO} className="mg-char-first">
        <div className="mg-grid">
          <MgField label="Character name">
            {(id) => (
              <span className="mg-name">
                <TextBox id={id} value={name ?? snapshot.name} onChange={(e) => setName(e.target.value)}
                  onBlur={() => { if (name !== null && name.trim() && name !== snapshot.name) rename(name.trim()); setName(null); }} />
                <button type="button" className="mg-name-dice" aria-label="Random name" title="Random name" onClick={() => rename(randomName())}><MgIcon name="dice" /></button>
              </span>
            )}
          </MgField>
          <DetailField k="gender" label="Gender" list={GENDERS} />
          <DetailField k="alignment" label="Alignment" choices={alignments} ok />
          <DetailField k="playerName" label="Player name" />
          <MgField label="Current experience">
            {() => <XpField value={snapshot.level.xp} onChange={(xp) => void mutate((api, cid) => api.setXp({ characterId: cid, xp }))} />}
          </MgField>
          <DetailField k="deity" label="Deity (optional)" list={deities} />
        </div>
      </MgSection>
      <MgSection title="Appearance" sub="Describe how your character looks in the fields below." className="mg-gap mg-char-appearance">
        <div className="mg-grid">
          <DetailField k="age" label="Age" />
          <DetailField k="height" label="Height" />
          <DetailField k="weight" label="Weight" />
          <DetailField k="eyes" label="Eyes" list={EYES} />
          <DetailField k="skin" label="Skin" list={SKIN} />
          <DetailField k="hair" label="Hair" list={HAIR} />
        </div>
      </MgSection>
      <MgSection title="Additional features" sub="Note any extra traits your character has that do not come from their race, class, or background." className="mg-gap-s">
        <DetailField k="features" label="Additional features" area />
      </MgSection>
    </div>
  );
}
