import type { CharacterSnapshot, ClassSpellcasting, PlayState, SpellEntry } from "../../api/contract";
import { AddedMark, EmptyState, useToast } from "../../components";
import { Expander } from "../../components/controls";
import { Tag } from "../ai/Panes";
import { type CastChoice, castChoices, choiceLabel, defaultChoice } from "./cast";
import { ElementDescription } from "./PlaySplit";
import { usePlayMutate } from "./playQueue";
import { D20Button, PressButton } from "./rolls/RollButton";
import { ConcentrationBanner } from "./SpellSlotsCard";

const levelName = (level: number) => (level === 0 ? "Cantrips" : `Level ${level}`);

/** Cast: spend the slot through the spell tracker's command, then concentrate through the picker's command (GitHub #2). */
function useCast(play: PlayState) {
  const run = usePlayMutate();
  const toast = useToast();
  return async (spell: SpellEntry, choice: CastChoice) => {
    // a refused spend (the slot went in the meantime) shows the host's error and casts nothing
    const spent = choice.kind === "slot"
      ? await run((api, characterId) => api.spendSpellSlot({ characterId, level: choice.level, pact: choice.pact }))
      : true;
    if (!spent) return;
    const concentrates = spell.concentration && play.concentrationSpells.some((c) => c.id === spell.id);
    if (concentrates) await run((api, characterId) => api.setConcentration({ characterId, spellId: spell.id }));
    const how = choice.kind === "ritual" ? "as a ritual" : choice.pact ? `with a Pact Magic slot (level ${choice.level})` : `with a level ${choice.level} slot`;
    const ended = concentrates && play.concentration && play.concentration.id !== spell.id ? ` ${play.concentration.name} ends.` : "";
    toast.push(`Cast ${spell.name} ${how}.${concentrates ? ` Concentrating on ${spell.name}.` : ""}${ended}`);
  };
}

function CastButton({ spell, play }: { spell: SpellEntry; play: PlayState }) {
  const cast = useCast(play);
  const choices = castChoices(spell, play);
  const preferred = defaultChoice(spell, choices);
  const what = preferred ? choiceLabel(preferred).replace(/ \(\d+ left\)$/, "").toLowerCase() : "choose a slot";
  return (
    <PressButton icon={false} disabled={choices.length === 0} label={choices.length === 0 ? `Cast ${spell.name}: no slots left` : `Cast ${spell.name}, ${what}`}
      menuLabel={`Cast ${spell.name}`} items={choices.map((c) => ({ label: choiceLabel(c), run: () => void cast(spell, c) }))}
      onTap={(openMenu) => (preferred ? void cast(spell, preferred) : openMenu())}>
      Cast
    </PressButton>
  );
}

function SpellRow({ spell, play, selected, onSelect, castable }: { spell: SpellEntry; play: PlayState; selected: boolean; onSelect: (id: string) => void; castable: boolean }) {
  return (
    <li className={`v5-line pr-line ${selected ? "is-selected" : ""}`}>
      <button type="button" className="v5-row grow pr-name" aria-current={selected} aria-label={`${spell.name}, description`} onClick={() => onSelect(spell.id)}>
        <span className="grow">{spell.name}{spell.grantedFrom && <> <span className="meta">({spell.grantedFrom})</span></>}{spell.added && <> <AddedMark /></>}</span>
        {spell.concentration && <Tag title="Concentration">C</Tag>}
        {spell.ritual && <Tag title="Ritual">R</Tag>}
        <span className="meta">{spell.casting?.castingTime ?? ""}</span>
      </button>
      {castable && spell.level > 0 && <span className="v5-ctrls pr-rolls"><CastButton spell={spell} play={play} /></span>}
    </li>
  );
}

function slotNote(level: number, play: PlayState): string {
  const row = play.spellSlots.find((s) => s.level === level);
  const pact = play.pactSlots?.level === level ? play.pactSlots : null;
  return [row ? `${row.max - row.used}/${row.max} slots` : null, pact ? `Pact Magic ${pact.max - pact.used}/${pact.max}` : null].filter(Boolean).join(" · ");
}

/**
 * One block's ready spells. Aurora's "Other Spells" (`c.other`: spells granted with no spellcasting name) show where each
 * came from and have no Cast button: they are cast at will, as rituals or with the feature's own uses, not a class slot.
 */
export function CasterCard({ c, play, selectedId, onSelect }: { c: ClassSpellcasting; play: PlayState; selectedId: string | null; onSelect: (id: string) => void }) {
  const ready = c.spells.filter((x) => x.ready);
  const levels = [...new Set(ready.map((x) => x.level))].sort((a, b) => a - b);
  return (
    <Expander title={c.other ? c.className : `${c.className} spells`} headerExtra={<span>{ready.length}</span>}>
      {/* a block without a casting ability (an item's spells when the content names none) has no attack or DC to show */}
      {c.ability && (
        <ul className="v5-list" aria-label={`${c.className} spellcasting`}>
          <li className="v5-line pr-line">
            <span className="v5-row static grow"><span className="grow">Spell attack</span><span className="meta">Save DC {c.saveDc}</span></span>
            <span className="v5-ctrls pr-rolls"><D20Button what={`${c.className} spell attack`} modifier={c.attackRoll} prefix="Hit" /></span>
          </li>
        </ul>
      )}
      {ready.length === 0 && <EmptyState title="No spells ready">Prepare spells on Magic.</EmptyState>}
      {levels.map((lvl) => (
        <div key={lvl}>
          <div className="v5-group pr-level">{levelName(lvl)}{!c.other && lvl > 0 && slotNote(lvl, play) ? <span className="pr-level-slots"> · {slotNote(lvl, play)}</span> : null}</div>
          <ul className="v5-list" aria-label={`${c.className} ${levelName(lvl).toLowerCase()}`}>
            {ready.filter((x) => x.level === lvl).map((x) => <SpellRow key={x.id} spell={x} play={play} selected={x.id === selectedId} onSelect={onSelect} castable={!c.other} />)}
          </ul>
        </div>
      ))}
    </Expander>
  );
}

/** Spells sub-tab (GitHub #14): the spells ready to cast by level, with Cast spending a slot; concentration pinned on top. */
export function SpellsPanel({ s, selectedId, onSelect }: { s: CharacterSnapshot; selectedId: string | null; onSelect: (id: string) => void }) {
  return (<>
    <ConcentrationBanner concentration={s.play.concentration} />
    {s.spellcasting.classes.map((c) => <CasterCard key={c.classSlotId} c={c} play={s.play} selectedId={selectedId} onSelect={onSelect} />)}
  </>);
}

/** Description pane body for a spell. */
export function SpellDetail({ s, spellId }: { s: CharacterSnapshot; spellId: string | null }) {
  const spell = s.spellcasting.classes.flatMap((c) => c.spells).find((x) => x.id === spellId);
  if (!spell) return (<><h1 className="v5-desc-title">Spells</h1><p className="v5-lead">Select a spell to read its description. Cast spends a slot.</p></>);
  const lead = [spell.level === 0 ? `${spell.school} cantrip` : `Level ${spell.level} ${spell.school}`, spell.concentration ? "Concentration" : null, spell.ritual ? "Ritual" : null]
    .filter(Boolean).join(" · ");
  return <ElementDescription elementId={spell.id} lead={<><h1 className="v5-desc-title">{spell.name}</h1><p className="v5-lead">{lead}</p></>} />;
}
