import { ABILITY_KEYS } from "../../api/contract";
import { TextBox } from "../../components";
import { ABILITY_LABEL } from "../../lib/format";
import { useCharacter } from "../../state/CharacterContext";
import { MgField, MgSection } from "./Field";

export function CompanionTab() {
  const { snapshot, mutate } = useCharacter();
  const c = snapshot?.companion;
  if (!c) return null;
  return (
    <div className="mg-page">
      <MgSection title="Companion" sub={c.creature?.name ?? "Your companion's name and statistics."}>
        <div className="mg-grid">
          <MgField label="Companion name">
            {(id) => <TextBox id={id} defaultValue={c.name} onBlur={(e) => void mutate((api, cid) => api.updateCompanion({ characterId: cid, patch: { name: e.target.value } }))} />}
          </MgField>
        </div>
        <div className="mg-tiles" aria-label="Companion statistics" style={{ marginTop: 10 }}>
          <div className="mg-tile"><span>CR</span><b>{c.challenge}</b></div>
          <div className="mg-tile"><span>AC</span><b>{c.armorClassNote ?? c.armorClass}</b></div>
          <div className="mg-tile"><span>HP</span><b>{c.hpNote ?? c.hp}</b></div>
          <div className="mg-tile"><span>Speed</span><b>{c.speed} ft</b></div>
          {ABILITY_KEYS.map((k) => <div key={k} className="mg-tile"><span>{ABILITY_LABEL[k].slice(0, 3)}</span><b>{c.abilities[k]}</b></div>)}
        </div>
      </MgSection>
    </div>
  );
}
