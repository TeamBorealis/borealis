import { render, screen, within } from "@testing-library/react";
import { AppProviders } from "../AppProviders";
import { afterEach, describe, expect, it } from "vitest";
import type { ClassSpellcasting, PlayState, SpellEntry } from "../api/contract";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { RollProvider } from "../pages/play/rolls/RollContext";
import { CasterCard } from "../pages/play/SpellsPanel";
import { ClassSpellGrid } from "../pages/spells/ClassSpellGrid";

/**
 * U20: Play > Spells marks prepared and always-prepared spells with the sheet's spell table legend: a dot for prepared,
 * a diamond for always prepared. Cantrips, a known caster's spells and Other Spells get no mark.
 */
const spell = (id: string, name: string, level: number, prepared: boolean, alwaysPrepared: boolean, grantedFrom?: string): SpellEntry => ({
  id, name, level, school: "Evocation", prepared, alwaysPrepared, origin: "class", slotId: null, canReplace: false,
  concentration: false, ritual: false, casting: null, sourceName: "", ready: true, grantedFrom,
});

const caster = (over: Partial<ClassSpellcasting>): ClassSpellcasting => ({
  classSlotId: "main", className: "Cleric", ability: "wis", saveDc: 13, attackBonus: 5, attackRoll: 5, model: "prepared",
  cantripsMax: 3, cantripsKnown: 1, preparedMax: 4, preparedCount: 1, maxSpellLevel: 1, pact: null, spells: [], ...over,
});

const play = {
  spellSlots: [{ level: 1, max: 2, used: 0 }], pactSlots: null, concentration: null, concentrationSpells: [],
} as unknown as PlayState;

/** The mark of the row whose description button is `name` (null when the row has none). */
const markOf = (name: string) => {
  const row = screen.getByRole("button", { name: `${name}, description` }).closest("li")!;
  const mark = within(row).queryByRole("img");
  return mark ? { label: mark.getAttribute("aria-label"), always: mark.classList.contains("always") } : null;
};

afterEach(() => { setPhoneForTests(false); });

/** A caster card with the Play providers its roll and Cast buttons need. */
const show = (c: ClassSpellcasting) => render(
  <AppProviders api={new MockApi()} restore={false}>
    <RollProvider characterId="test"><CasterCard c={c} play={play} selectedId={null} onSelect={() => undefined} /></RollProvider>
  </AppProviders>,
);

describe("Play: prepared marks (U20)", () => {
  it("marks a prepared spell with a dot and an always-prepared one with a diamond; cantrips get none", () => {
    const c = caster({
      spells: [spell("c", "Sacred Flame", 0, true, true), spell("p", "Bless", 1, true, false), spell("a", "Cure Wounds", 1, true, true)],
    });
    show(c);
    expect(markOf("Bless")).toEqual({ label: "Prepared", always: false });
    expect(markOf("Cure Wounds")).toEqual({ label: "Always prepared", always: true });
    expect(markOf("Sacred Flame")).toBeNull();
    expect(screen.getAllByRole("img", { name: "Prepared" })).toHaveLength(1);
    expect(screen.getAllByRole("img", { name: "Always prepared" })).toHaveLength(1);
  });

  it("leaves a known caster's learned spells unmarked and still marks its always-prepared ones", () => {
    const c = caster({ className: "Bard", model: "known", spells: [spell("k", "Faerie Fire", 1, false, false), spell("a", "Command", 1, true, true)] });
    show(c);
    expect(markOf("Faerie Fire")).toBeNull();
    expect(markOf("Command")).toEqual({ label: "Always prepared", always: true });
  });

  it("gives Other Spells no mark and no mark column", () => {
    const c = caster({ className: "Other Spells", ability: "" as ClassSpellcasting["ability"], other: true, spells: [spell("o", "Mage Armor", 1, false, false, "Veil")] });
    const { container } = show(c);
    expect(markOf("Mage Armor")).toBeNull();
    expect(container.querySelector(".pr-prep")).toBeNull();
  });

  it("keeps the same marks on a phone", () => {
    setPhoneForTests(true);
    const c = caster({ spells: [spell("p", "Bless", 1, true, false), spell("a", "Cure Wounds", 1, true, true)] });
    show(c);
    expect(markOf("Bless")?.label).toBe("Prepared");
    expect(markOf("Cure Wounds")?.label).toBe("Always prepared");
  });
});

describe("Magic spell table tick labels", () => {
  it("labels an always-prepared spell's tick (cantrips included) Always prepared, not Prepared", () => {
    const c = caster({
      className: "Bard", model: "known", preparedMax: 0,
      spells: [spell("c", "Vicious Mockery", 0, true, true), spell("k", "Faerie Fire", 1, true, false), spell("a", "Command", 1, true, true)],
    });
    render(<AppProviders api={new MockApi()} restore={false}><ClassSpellGrid cls={c} picked={null} onPick={() => undefined} /></AppProviders>);
    const table = screen.getByRole("table", { name: "Bard spells" });
    const tick = (id: string) => table.querySelector(`tr[data-spell-id="${id}"] td.sp-ck .sp-mark`)?.getAttribute("aria-label");
    expect(tick("a")).toBe("Always prepared");
    expect(tick("c")).toBe("Always prepared");
    expect(tick("k")).toBe("Prepared");
  });
});
