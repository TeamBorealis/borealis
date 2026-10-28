import { useState } from "react";
import type { ItemSummary, PlayState, PlayWildShape, Ruleset, SelectionSlot } from "../../api/contract";
import { Alert, Button, Chip, EmptyState, Expander } from "../../components";
import "./equipment.css";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useAsync } from "../../state/useAsync";
import { searchAllItems } from "./EquipmentTab";

/** Weapon Mastery slots are `weapon` selects titled after the feature (backend `is_mastery_slot`); their count is the capacity. */
export const masterySlots = (slots: SelectionSlot[]): SelectionSlot[] => slots.filter((s) => s.elementType === "weapon" && s.title.toLowerCase().includes("mastery"));

export function MasteryChooser({ capacity, play, weapons }: { capacity: number; play: PlayState; weapons: ItemSummary[] }) {
  const { mutate } = useCharacter();
  const chosen = play.masteries.map((m) => m.weapon);
  const isChosen = (w: ItemSummary) => chosen.includes(w.id) || chosen.includes(w.name);
  const textFor = (w: ItemSummary) => play.masteries.find((m) => m.weapon === w.id || m.weapon === w.name);
  const full = chosen.length >= capacity;
  const save = (ids: string[]) => void mutate((api, id) => api.chooseWeaponMasteries({ characterId: id, weaponIds: ids }));
  const toggle = (w: ItemSummary) => {
    const current = play.masteries.map((m) => weapons.find((x) => x.id === m.weapon || x.name === m.weapon)?.id ?? m.weapon);
    save(isChosen(w) ? current.filter((id) => id !== w.id) : [...current, w.id]);
  };
  return (
    <Expander title="Weapon Mastery" headerExtra={<Chip tone={full ? "success" : "accent"}>{chosen.length} of {capacity} chosen</Chip>}>
      <ul className="list" aria-label="Weapons">
        {weapons.map((w) => {
          const on = isChosen(w);
          const m = textFor(w);
          return (
            <li key={w.id} className="resource-row">
              <span className="grow"><strong>{w.name}</strong> <span className="mastery-src muted small">{w.sourceName}</span><span className="muted small" style={{ display: "block" }}>{m ? `${m.property}: ${m.text}` : w.shortDescription}</span></span>
              <Button small variant={on ? "primary" : "default"} aria-pressed={on} disabled={!on && full} aria-label={`${on ? "Unmaster" : "Master"} ${w.name}`} onClick={() => toggle(w)}>{on ? "Mastered" : "Master"}</Button>
            </li>
          );
        })}
      </ul>
    </Expander>
  );
}

export function WildShapeChooser({ ws }: { ws: PlayWildShape }) {
  const { mutate } = useCharacter();
  const known = ws.known.map((f) => f.id);
  const full = known.length >= ws.maxForms;
  const save = (ids: string[]) => void mutate((api, id) => api.setWildShapeForms({ characterId: id, formIds: ids }));
  return (
    <Expander title="Wild Shape" headerExtra={<Chip tone={full ? "success" : "accent"}>{known.length} of {ws.maxForms} forms</Chip>}>
      <p className="muted small">Up to CR {ws.maxCr}{ws.flyAllowed ? ", flying allowed" : ", no flying"} · {ws.durationHours} h · {ws.tempHp} temp HP</p>
      {ws.invalid.length > 0 && <Alert tone="warning">No longer valid: {ws.invalid.join(", ")}</Alert>}
      <ul className="list" aria-label="Wild Shape forms">
        {ws.options.map((f) => {
          const on = known.includes(f.id);
          return (
            <li key={f.id} className="resource-row">
              <span className="grow"><strong>{f.name}</strong><span className="muted small" style={{ display: "block" }}>CR {f.cr} · AC {f.ac} · HP {f.hp} · {f.speed}</span></span>
              <Button small variant={on ? "primary" : "default"} aria-pressed={on} disabled={!on && full} aria-label={`${on ? "Forget" : "Learn"} ${f.name}`} onClick={() => save(on ? known.filter((x) => x !== f.id) : [...known, f.id])}>{on ? "Known" : "Learn"}</Button>
            </li>
          );
        })}
      </ul>
    </Expander>
  );
}

export function PackageChooser({ packages }: { packages: PlayState["equipmentPackages"] }) {
  const { mutate } = useCharacter();
  const [picked, setPicked] = useState<Record<string, string>>({});
  return (
    <Expander title="Starting equipment">
      {packages.length === 0 ? <EmptyState title="No starting equipment to choose" /> : (
        <div className="stack">
          {packages.map((p) => {
            const group = `pkg-${p.package}`;
            const sel = p.chosen ?? picked[p.package] ?? "";
            return (
              <fieldset key={p.package} className="pkg" disabled={!!p.chosen}>
                <legend><strong>{p.name}</strong>{p.owner ? <span className="muted small"> · {p.owner}</span> : null}{p.chosen ? <> <Chip tone="success">Option {p.chosen} taken</Chip></> : null}</legend>
                {p.options.map((o) => (
                  <label key={o.id} className="check">
                    <input type="radio" name={group} checked={sel === o.id} onChange={() => setPicked((cur) => ({ ...cur, [p.package]: o.id }))} />
                    <span className="check-text"><span>Option {o.id}</span><span className="hint">{[...o.items.map((it) => (it.qty > 1 ? `${it.qty} x ${it.name}` : it.name)), ...(o.coins.gp > 0 ? [`${o.coins.gp} gp`] : [])].join(", ")}</span></span>
                  </label>
                ))}
                {!p.chosen && <Button variant="primary" disabled={!sel} onClick={() => void mutate((api, id) => api.chooseEquipmentPackage({ characterId: id, packageId: p.package, option: sel }))}>Take option {sel}</Button>}
              </fieldset>
            );
          })}
        </div>
      )}
    </Expander>
  );
}

/** B-55 W3: the weapons a character may master: its own edition (Weapon Mastery is a 2024 rule; edition-neutral counts), its sources. */
export const masteryWeapons = (items: readonly ItemSummary[], ruleset: Ruleset): ItemSummary[] => items.filter((w) => w.rulesets.includes(ruleset));

export function LoadoutTab() {
  const { snapshot } = useCharacter();
  const api = useApi();
  const capacity = snapshot ? masterySlots(snapshot.slots).length : 0;
  const id = snapshot?.id ?? "";
  // The host applies the character's edition and source restrictions; refetch when they change.
  const scopeKey = snapshot ? JSON.stringify([snapshot.ruleset, snapshot.sourceRestrictions]) : "";
  const weapons = useAsync(
    () => (capacity > 0 && id ? searchAllItems(api, { category: "Weapons", characterId: id }) : Promise.resolve({ total: 0, items: [] as ItemSummary[] })),
    [api, capacity, id, scopeKey],
  );
  if (!snapshot) return null;
  const { play } = snapshot;
  return (
    <div className="eq-load page-pad">
      <PackageChooser packages={play.equipmentPackages} />
      {capacity > 0 && <MasteryChooser capacity={capacity} play={play} weapons={masteryWeapons(weapons.data?.items ?? [], snapshot.ruleset)} />}
      {play.wildShape && <WildShapeChooser ws={play.wildShape} />}
      {capacity === 0 && !play.wildShape && <p className="muted small">Weapon Mastery and Wild Shape appear here when the character's classes grant them.</p>}
    </div>
  );
}
