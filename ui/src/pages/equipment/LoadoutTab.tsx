import { useState } from "react";
import type { Api, ElementSummary, PlayMastery, PlayState, PlayWildShape, SelectionSlot } from "../../api/contract";
import { Alert, Button, Chip, EmptyState, Expander } from "../../components";
import "./equipment.css";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useAsync } from "../../state/useAsync";
import { groupKey } from "../../lib/versions";
import { fetchAllSlotOptions } from "../builder/usePickerRows";

/** Weapon Mastery slots are `weapon` selects titled after the feature (backend `is_mastery_slot`); their count is the capacity. */
export const masterySlots = (slots: SelectionSlot[]): SelectionSlot[] => slots.filter((s) => s.elementType === "weapon" && s.title.toLowerCase().includes("mastery"));

/** One row of the Weapon Mastery list: a kind of weapon the slots offer, or a saved choice they no longer offer. */
export interface MasteryRow { id: string; name: string; sourceName?: string; shortDescription?: string; stale?: boolean }

/**
 * The list: every weapon the character's Weapon Mastery slots offer (one per name), then each saved choice they no longer
 * offer, kept so it can be seen and dropped but never offered anew.
 */
export function masteryRows(offered: readonly ElementSummary[], masteries: readonly PlayMastery[]): MasteryRow[] {
  const known = (m: PlayMastery) => offered.some((w) => w.id === m.weapon || w.name === m.weapon);
  const stale = masteries.filter((m) => !known(m)).map((m) => ({ id: m.weapon, name: m.name || m.weapon, stale: true }));
  return [...offered, ...stale];
}

export function MasteryChooser({ capacity, play, weapons }: { capacity: number; play: PlayState; weapons: MasteryRow[] }) {
  const { mutate } = useCharacter();
  const chosen = play.masteries.map((m) => m.weapon);
  const isChosen = (w: MasteryRow) => chosen.includes(w.id) || chosen.includes(w.name);
  const textFor = (w: MasteryRow) => play.masteries.find((m) => m.weapon === w.id || m.weapon === w.name);
  const full = chosen.length >= capacity;
  const save = (ids: string[]) => void mutate((api, id) => api.chooseWeaponMasteries({ characterId: id, weaponIds: ids }));
  const toggle = (w: MasteryRow) => {
    const current = play.masteries.map((m) => weapons.find((x) => x.id === m.weapon || x.name === m.weapon)?.id ?? m.weapon);
    save(isChosen(w) ? current.filter((id) => id !== w.id) : [...current, w.id]);
  };
  return (
    <Expander title="Weapon Mastery" headerExtra={<Chip tone={full ? "success" : "accent"}>{chosen.length} of {capacity} chosen</Chip>}>
      <ul className="list" aria-label="Weapons">
        {weapons.map((w) => {
          const on = isChosen(w);
          const m = textFor(w);
          // A saved choice the slots no longer offer can only be dropped.
          if (w.stale && !on) return null;
          const detail = w.stale ? "No longer available for Weapon Mastery; unmaster it to choose another weapon." : m ? `${m.property}: ${m.text}` : w.shortDescription;
          return (
            <li key={w.id} className="resource-row">
              <span className="grow"><strong>{w.name}</strong> {w.sourceName && <span className="mastery-src muted small">{w.sourceName}</span>}<span className="muted small" style={{ display: "block" }}>{detail}</span></span>
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

/**
 * The kinds of weapon the character may master: the options of its Weapon Mastery slots, one per name. The engine applies
 * each class's limits (SRD 5.2.1: Fighter any Simple or Martial weapon, Barbarian melee ones, Paladin, Ranger and Rogue those
 * they are proficient with; always a base weapon with a mastery property), the edition and the source restrictions.
 */
export async function masteryOptions(api: Api, characterId: string, slots: readonly SelectionSlot[]): Promise<ElementSummary[]> {
  const pages = await Promise.all(slots.map((s) => fetchAllSlotOptions(api, { characterId, slotId: s.id, query: "" })));
  const rows = new Map<string, ElementSummary>();
  for (const o of pages.flatMap((p) => p.items)) {
    const key = groupKey(o);
    if (!rows.has(key) && !o.disabledReason) rows.set(key, o);
  }
  return [...rows.values()].sort((a, b) => a.name.localeCompare(b.name));
}

export function LoadoutTab() {
  const { snapshot } = useCharacter();
  const api = useApi();
  const slots = snapshot ? masterySlots(snapshot.slots) : [];
  const capacity = slots.length;
  const id = snapshot?.id ?? "";
  // The slots' options change with the classes, picks, edition and source restrictions.
  const slotKey = snapshot ? JSON.stringify([slots.map((s) => [s.id, s.chosen?.id ?? ""]), snapshot.ruleset, snapshot.sourceRestrictions]) : "";
  const weapons = useAsync(
    () => (capacity > 0 && id ? masteryOptions(api, id, slots) : Promise.resolve([] as ElementSummary[])),
    [api, capacity, id, slotKey], // eslint-disable-line react-hooks/exhaustive-deps
  );
  if (!snapshot) return null;
  const { play } = snapshot;
  return (
    <div className="eq-load page-pad">
      <PackageChooser packages={play.equipmentPackages} />
      {capacity > 0 && <MasteryChooser capacity={capacity} play={play} weapons={masteryRows(weapons.data ?? [], play.masteries)} />}
      {play.wildShape && <WildShapeChooser ws={play.wildShape} />}
      {capacity === 0 && !play.wildShape && <p className="muted small">Weapon Mastery and Wild Shape appear here when the character's classes grant them.</p>}
    </div>
  );
}
