import { useState } from "react";
import { TextBox } from "../../components";
import { randomName } from "../../lib/format";
import { useCharacter } from "../../state/CharacterContext";
import { DetailField } from "./DetailField";
import { MgField, MgSection } from "./Field";
import { MgIcon } from "./mgIcons";
import { XpField } from "./XpField";
import "./manage-fx15.css";

const ALIGNMENTS = ["Lawful Good", "Neutral Good", "Chaotic Good", "Lawful Neutral", "True Neutral", "Chaotic Neutral", "Lawful Evil", "Neutral Evil", "Chaotic Evil"];
const GENDERS = ["Male", "Female", "Non-binary", "Construct"];
const EYES = ["Amber", "Black", "Blue", "Brown", "Gray", "Green", "Hazel", "Violet"];
const SKIN = ["Pale", "Fair", "Light", "Tan", "Olive", "Brown", "Dark", "Ebony", "Ashen", "Green", "Gray"];
const HAIR = ["Black", "Blond", "Brown", "Auburn", "Red", "Gray", "White", "Silver", "Bald"];
const DEITIES = ["Apollo", "Athena", "Brigid", "Demeter", "Freya", "Hades", "Hermes", "Isis", "Odin", "Osiris", "Ra", "Thor", "Tyr", "Zeus"];

const CHARACTER_INTRO = "A hero is far more than a species and a class. Here you record who they are: their name and looks, where they came from, the languages they speak, the things they believe in, and the temperament and ideals that set them apart from every other adventurer you will ever meet along the road.";

export function CharacterTab() {
  const { snapshot, mutate } = useCharacter();
  const [name, setName] = useState<string | null>(null);
  if (!snapshot) return null;
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
          <DetailField k="gender" label="Gender" choices={GENDERS} />
          <DetailField k="alignment" label="Alignment" choices={ALIGNMENTS} ok />
          <DetailField k="playerName" label="Player name" />
          <MgField label="Current experience">
            {() => <XpField value={snapshot.level.xp} onChange={(xp) => void mutate((api, cid) => api.setXp({ characterId: cid, xp }))} />}
          </MgField>
          <DetailField k="deity" label="Deity (optional)" choices={DEITIES} />
        </div>
      </MgSection>
      <MgSection title="Appearance" sub="Describe how your character looks in the fields below." className="mg-gap mg-char-appearance">
        <div className="mg-grid">
          <DetailField k="age" label="Age" />
          <DetailField k="height" label="Height" />
          <DetailField k="weight" label="Weight" />
          <DetailField k="eyes" label="Eyes" choices={EYES} />
          <DetailField k="skin" label="Skin" choices={SKIN} />
          <DetailField k="hair" label="Hair" choices={HAIR} />
        </div>
      </MgSection>
      <MgSection title="Additional features" sub="Note any extra traits your character has that do not come from their race, class, or background." className="mg-gap-s">
        <DetailField k="features" label="Additional features" area />
      </MgSection>
    </div>
  );
}
