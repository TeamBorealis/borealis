import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ClassSpellcasting, PlayState, SpellEntry } from "../api/contract";
import { CasterCard } from "../pages/play/SpellsPanel";

/**
 * #38: spells a feat picks in Aurora-format content (selects with no spellcasting name) get a block of their own, which
 * has no casting ability when the content names none. Play lists its spells without a made-up "Save DC 0" / "Hit +0" row.
 */
const cantrip: SpellEntry = {
  id: "ID_T_PRESTIDIGITATION", name: "Prestidigitation", level: 0, school: "Transmutation", prepared: true, alwaysPrepared: true,
  origin: "feat", slotId: null, canReplace: false, concentration: false, ritual: false, casting: null, sourceName: "", ready: true,
};

const block: ClassSpellcasting = {
  classSlotId: "caster:spark", className: "Spark Initiate", ability: "" as ClassSpellcasting["ability"], saveDc: 0, attackBonus: 0,
  attackRoll: 0, model: "known", cantripsMax: 0, cantripsKnown: 1, preparedMax: 0, preparedCount: 0,
  maxSpellLevel: 0, pact: null, spells: [cantrip],
};

const play = { spellSlots: [], pactSlots: null, concentration: null, concentrationSpells: [] } as unknown as PlayState;

describe("Play spells of a block without a casting ability", () => {
  it("lists the spells and shows no spell attack or save DC", () => {
    render(<CasterCard c={block} play={play} selectedId={null} onSelect={() => undefined} />);
    expect(screen.getByText("Spark Initiate spells")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Prestidigitation, description" })).toBeTruthy();
    expect(screen.queryByText("Spell attack")).toBeNull();
    expect(screen.queryByText(/Save DC/)).toBeNull();
  });
});
