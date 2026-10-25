import { type ReactNode, useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router";
import type { ClassSpellcasting, PactSlotRow, SelectionSlot, SpellSlotRow } from "../../api/contract";
import { TabPanel, Tabs, signed } from "../../components";
import { RequireCharacter } from "../../components/RequireCharacter";
import { ABILITY_LABEL } from "../../lib/format";
import { useCharacter } from "../../state/CharacterContext";
import { usePhone } from "../../lib/phone";
import { KnownSpells, SpellGrid } from "./SpellGrid";
import { ClassSpellGrid } from "./ClassSpellGrid";
import { Band, TwoPane } from "./TwoPane";
import { EMPTY_FILTER, filterClass, SpellFilterBar, type SpellFilterState } from "./SpellFilter";
import { catalogueClassOf, SlotsTable } from "./SlotsTable";
import { SlotOptionsGrid } from "./SlotOptionsGrid";
import "./magic.css";

const OVERVIEW = "overview";

function SlotTile({ label, total, aria }: { label: string; total: number; aria: string }) {
  return (
    <div className="mg-slot" role="group" aria-label={aria}>
      <span className="k">{label}</span>
      <span className="pips" aria-hidden="true">{Array.from({ length: total }, (_, i) => <i key={i} />)}</span>
      <span className="n">{total}</span>
    </div>
  );
}

/** Spell slot tiles, then the class's Pact Magic slots (a separate pool that recovers on a short rest). */
function SlotTiles({ rows, pact }: { rows: SpellSlotRow[]; pact?: PactSlotRow | null }) {
  if (rows.length === 0 && !pact) return null;
  return (
    <div className="mg-slots" aria-label="Spell slots">
      {rows.map((r) => <SlotTile key={r.level} label={`Level ${r.level}`} total={r.total} aria={`Level ${r.level}: ${r.total} slots`} />)}
      {pact && <SlotTile label={`Pact level ${pact.level} (short rest)`} total={pact.total}
        aria={`Pact slots (short rest), level ${pact.level}: ${pact.total} slots`} />}
    </div>
  );
}

const classStats = (c: ClassSpellcasting): [string, string | number][] => [
  ["Ability:", ABILITY_LABEL[c.ability].slice(0, 3).toUpperCase()], ["Attack modifier:", signed(c.attackBonus)], ["Save DC:", c.saveDc],
  ["Cantrips:", `${c.cantripsKnown}/${c.cantripsMax}`], ["Prepared:", `${c.preparedCount}/${c.preparedMax}`],
];

/** One SpellcastingExpanderContent: the 4 px focus bar spans only this block (lit while it is the focused one). */
function ClassBlock({ cls, shownCls = cls, slotRows, focused, picked, onPick, full, catalogue, children }: {
  cls: ClassSpellcasting; catalogue?: ReactNode; shownCls?: ClassSpellcasting; slotRows: SpellSlotRow[]; focused: boolean; picked: string | null; onPick: (id: string) => void; full?: boolean; children?: ReactNode;
}) {
  return (
    <section className="sp-block" aria-label={`${cls.className} spellcasting`}>
      <span className={`sp-bar ${focused ? "on" : ""}`} aria-hidden="true" />
      <div className="sp-col">
        <div className="sp-main-wrap">
          <Band title={cls.className} stats={classStats(cls)}><SlotTiles rows={slotRows} pact={cls.pact} /></Band>
          {catalogue ?? <SpellGrid cls={shownCls} picked={picked} onPick={onPick} />}
        </div>
        {full && <KnownSpells cls={cls} picked={picked} onPick={onPick} />}
        {children}
      </div>
    </section>
  );
}

function ClassMagic({ cls, picked, onPick }: { cls: ClassSpellcasting; picked: string | null; onPick: (id: string) => void }) {
  const { snapshot } = useCharacter();
  if (!snapshot) return null;
  const rows = snapshot.spellcasting.multiclassSlots ?? snapshot.spellcasting.slots;
  return <ClassBlock cls={cls} slotRows={rows} focused picked={picked} onPick={onPick} catalogue={<ClassSpellGrid cls={cls} picked={picked} onPick={onPick} />} />;
}

interface OverviewProps {
  picked: string | null; onPick: (id: string) => void; filter: SpellFilterState; onFilter: (f: SpellFilterState) => void;
  current: string | null; onCurrent: (slotId: string) => void;
}

function Overview({ picked, onPick, filter, onFilter, current, onCurrent }: OverviewProps) {
  const { snapshot, mutate } = useCharacter();
  if (!snapshot) return null;
  const sc = snapshot.spellcasting;
  const rows = sc.multiclassSlots ?? sc.slots;
  const focusedId = (sc.classes.find((c) => c.spells.some((x) => x.id === picked)) ?? sc.classes[0])?.classSlotId;
  const slots = snapshot.slots.filter((s) => s.tab === "spells");
  const hot: SelectionSlot | null = slots.find((s) => s.id === current) ?? null;
  const choose = (slotId: string, elementId: string) => {
    onPick(elementId);
    void mutate((api, id) => api.applySelection({ characterId: id, slotId, elementId }));
  };
  const hotClass = hot ? catalogueClassOf(hot, sc.classes) : undefined;
  const catalogue = (c: ClassSpellcasting) => hot && c.classSlotId === hotClass
    ? <SlotOptionsGrid characterId={snapshot.id} cls={c} slot={hot} picked={picked} onPick={onPick} onChoose={choose} /> : undefined;
  return (
    <>
      <SpellFilterBar classes={sc.classes} filter={filter} onChange={onFilter} />
      {sc.classes.map((c) => <ClassBlock key={c.classSlotId} cls={c} catalogue={catalogue(c)} shownCls={filterClass(c, filter)} slotRows={rows} full focused={c.classSlotId === focusedId} picked={picked} onPick={onPick} />)}
      <SlotsTable slots={slots} classes={sc.classes} picked={picked} onPick={onPick} current={current} onCurrent={onCurrent} />
    </>
  );
}

function MagicBody() {
  const { snapshot } = useCharacter();
  const [active, setActive] = useState(OVERVIEW);
  const [picked, setPicked] = useState<string | null>(null);
  const [filter, setFilter] = useState<SpellFilterState>(EMPTY_FILTER);
  const [params] = useSearchParams();
  const wanted = params.get("slot");
  const [current, setCurrent] = useState<string | null>(wanted);
  const phone = usePhone();
  const seen = useRef(wanted);
  // NEXT SELECTION re-enters this page with a new ?slot=: highlight that row and show its options.
  useEffect(() => {
    if (wanted && wanted !== seen.current) { seen.current = wanted; setCurrent(wanted); setActive(OVERVIEW); }
  }, [wanted]);
  if (!snapshot) return null;
  const sc = snapshot.spellcasting;
  const tabs = [{ id: OVERVIEW, label: "Spellcasting" }, ...sc.classes.map((c) => ({ id: c.classSlotId, label: phone ? c.className : c.className.toUpperCase() }))];
  const cur = sc.classes.find((c) => c.classSlotId === active);
  const body = sc.classes.length === 0
    ? <p className="sp-empty">Your character doesn't have any spellcasting available here...</p>
    : cur ? <ClassMagic key={cur.classSlotId} cls={cur} picked={picked} onPick={setPicked} /> : <Overview picked={picked} onPick={setPicked} filter={filter} onFilter={setFilter} current={current} onCurrent={setCurrent} />;
  return (
    <>
      <Tabs inBand tabs={tabs} active={cur ? cur.classSlotId : OVERVIEW} onChange={(id) => { setActive(id); if (!filter.pinned) setFilter(EMPTY_FILTER); }} label="Magic sections" idPrefix="magic" />
      <h1 className="sr-only">Magic</h1>
      <TabPanel active={cur ? cur.classSlotId : OVERVIEW} idPrefix="magic">
        <TwoPane left={<div className="sp-page">{body}</div>} descEmpty={!picked} descId={picked} />
      </TabPanel>
    </>
  );
}

export function SpellsPage() {
  return <RequireCharacter><MagicBody /></RequireCharacter>;
}
