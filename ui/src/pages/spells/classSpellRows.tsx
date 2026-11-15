import { useMemo, useState } from "react";
import type { ClassSpellcasting, Ruleset, SpellEntry } from "../../api/contract";
import { groupKey, pickShown, type PreferredCopies, type VersionLike } from "../../lib/versions";
import { useCharacter } from "../../state/CharacterContext";
import { usePreferredCopies } from "../../state/preferredCopies";
import { setStatus } from "../../state/status";
import { usePickerUiState } from "../builder/pickerState";
import { VersionButton, VersionMenu } from "../builder/VersionMenu";

/**
 * GitHub #51 piece 2: a prepared caster's class spell list shows one row per name (B-55), like the pickers and the spell
 * slot catalogue. The engine lists every installed copy of a spell on the class's lists; the copies of one name share a
 * row, whose versions menu holds them.
 */

/** A class list spell as a version of its name's row. */
export const spellVersion = (s: SpellEntry): VersionLike => ({
  id: s.id, name: s.name, type: "spell", source: s.sourceName, sourceName: s.sourceName, rulesets: s.rulesets, bundled: s.bundled,
  level: s.level, school: s.school,
});

/**
 * The character has this copy (prepared, always prepared, a cantrip, chosen in a slot, added or granted): it keeps a row
 * of its own, as its own copy. The other copies of its name are listed in that row's menu.
 */
export const isHeld = (s: SpellEntry): boolean =>
  s.prepared || s.alwaysPrepared || s.level === 0 || s.slotId !== null || !!s.added || !!s.grantedFrom || !!s.restrictedBySource;

/** A copy prepared from the class list: its row can move the preparation to another copy. */
const isSwappable = (s: SpellEntry): boolean => s.prepared && !s.alwaysPrepared && s.level > 0 && s.slotId === null && !s.added && !s.grantedFrom;

/** Why a held row can't switch to another copy here: its slot or its grant decides the copy. */
const fixedReason = (s: SpellEntry): string => (s.slotId !== null ? "Switch it in its spell slot" : "Granted as the shown copy");

export interface ClassSpellRow {
  key: string;
  /** The entry the row shows (its prepare tick and source). */
  spell: SpellEntry;
  /** Every copy in the row's menu, the shown one included; one copy means no menu. */
  copies: SpellEntry[];
  /** The shown copy is the character's own (held): the menu marks it chosen. */
  held: boolean;
}

interface Group { held: SpellEntry[]; free: SpellEntry[] }

function groups(spells: readonly SpellEntry[]): Map<string, Group> {
  const map = new Map<string, Group>();
  for (const s of spells) {
    const key = groupKey(spellVersion(s));
    const g = map.get(key) ?? { held: [], free: [] };
    if (isHeld(s)) g.held.push(s);
    else if (!g.free.some((f) => f.id === s.id)) g.free.push(s);
    map.set(key, g);
  }
  return map;
}

/**
 * The rows of a class list, in the list's order (each row at the place of the entry it shows). A held copy keeps its own
 * row; the copies the character doesn't have join the first held row of their name, else form one row that shows the
 * last menu pick, then the copy always used (#51 piece 1), then the default order (sec. 1.2).
 */
export function classSpellRows(spells: readonly SpellEntry[], charEd: Ruleset, menuPicks: Readonly<Record<string, string>>, preferred: PreferredCopies): ClassSpellRow[] {
  const byKey = groups(spells);
  const shownOf = new Map<string, SpellEntry>();
  for (const [key, g] of byKey) {
    if (g.held.length > 0 || g.free.length === 0) continue;
    const shown = pickShown(g.free.map(spellVersion), charEd, null, menuPicks[key] ?? null, preferred);
    shownOf.set(key, g.free.find((f) => f.id === shown.id)!);
  }
  const out: ClassSpellRow[] = [];
  for (const s of spells) {
    const key = groupKey(spellVersion(s));
    const g = byKey.get(key)!;
    if (isHeld(s)) {
      const first = g.held[0] === s;
      out.push({ key: first ? key : `${key}\u0000${s.id}:${s.slotId ?? ""}`, spell: s, copies: first ? [s, ...g.free] : [s], held: true });
    } else if (shownOf.get(key) === s) {
      out.push({ key, spell: s, copies: g.free, held: false });
    }
  }
  return out;
}

/** How many rows a class list shows (the filter bar's count), without choosing the shown copies. */
export const classRowCount = (spells: readonly SpellEntry[]): number =>
  [...groups(spells).values()].reduce((n, g) => n + Math.max(g.held.length, g.free.length > 0 ? 1 : 0), 0);

/**
 * The class grids' rows and versions column. Picking another copy in the menu shows it; on a prepared row it moves the
 * preparation to that copy. A row of a copy the slot or a grant decides lists its other copies, but can't switch to them.
 */
export function useClassSpellRows(cls: ClassSpellcasting, onPick: (id: string) => void) {
  const { snapshot, mutate } = useCharacter();
  const charEd: Ruleset = snapshot?.ruleset ?? "dnd2024";
  const { preferred } = usePreferredCopies();
  const [ui, update] = usePickerUiState(`class-spells:${cls.classSlotId}`);
  const rows = useMemo(() => classSpellRows(cls.spells, charEd, ui.menuPicks, preferred), [cls.spells, charEd, ui.menuPicks, preferred]);
  const [menu, setMenu] = useState<{ key: string; anchor: HTMLElement } | null>(null);
  const menuRow = menu ? rows.find((r) => r.key === menu.key) ?? null : null;
  const versionColumn = rows.some((r) => r.copies.length > 1);

  const pickCopy = async (row: ClassSpellRow, id: string) => {
    menu?.anchor.focus();
    setMenu(null);
    update((s) => ({ ...s, menuPicks: { ...s.menuPicks, [row.key]: id } }));
    onPick(id);
    const to = row.copies.find((c) => c.id === id);
    if (!row.held || !to || id === row.spell.id || !isSwappable(row.spell)) return;
    const from = row.spell;
    const set = (spellId: string, prepared: boolean) =>
      mutate((api, characterId) => api.setSpellPrepared({ characterId, classSlotId: cls.classSlotId, spellId, prepared }));
    if (!(await set(from.id, false))) return;
    if (await set(to.id, true)) {
      setStatus(`${cls.className}: ${to.name} (${to.sourceName}) is prepared.`);
      return;
    }
    // the other copy couldn't be prepared: keep the one the character had
    await set(from.id, true);
    setStatus(`${cls.className}: ${to.name} (${to.sourceName}) couldn't be prepared; ${from.name} (${from.sourceName}) stays prepared.`);
  };

  const versionHeader = versionColumn ? <th scope="col" className="sp-ver"><span className="sr-only">Versions</span></th> : null;
  const versionCell = (row: ClassSpellRow) => versionColumn && (
    <td className="sp-ver" onMouseDown={(e) => e.stopPropagation()} onDoubleClick={(e) => e.stopPropagation()}>
      {row.copies.length > 1 && <VersionButton count={row.copies.length} rowName={row.spell.name} open={menu?.key === row.key}
        onOpen={(anchor) => setMenu(menu?.key === row.key ? null : { key: row.key, anchor })} />}
    </td>
  );
  const fixed = menuRow && menuRow.held && !isSwappable(menuRow.spell) ? fixedReason(menuRow.spell) : undefined;
  const versions: VersionLike[] = menuRow
    ? menuRow.copies.map((c) => ({ ...spellVersion(c), disabledReason: c.id === menuRow.spell.id ? undefined : fixed }))
    : [];
  const versionMenu = menu && menuRow && (
    <VersionMenu anchor={menu.anchor} rowName={menuRow.spell.name} versions={versions} charEd={charEd} shownId={menuRow.spell.id}
      chosenId={menuRow.held ? menuRow.spell.id : null} onPick={(v) => void pickCopy(menuRow, v.id)}
      onClose={(restore) => { setMenu(null); if (restore) menu.anchor.focus(); }} />
  );
  return { rows, columns: versionColumn ? 1 : 0, versionHeader, versionCell, versionMenu };
}
