import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ClassSpellcasting, PlayState, SpellEntry } from "../api/contract";
import { CasterCard } from "../pages/play/SpellsPanel";

/**
 * Spells Aurora-format content grants with no spellcasting name (an invocation's spell, a race's innate spells) form one
 * "Other Spells" block, as on Aurora's sheet. Play lists them with where they came from, and without a Cast button: they
 * are cast at will, as rituals or with the feature's own uses, never with a class's spell slot.
 */
const spell = (id: string, name: string, level: number, grantedFrom: string): SpellEntry => ({
  id, name, level, school: "Evocation", prepared: false, alwaysPrepared: false, origin: "class", slotId: null, canReplace: false,
  concentration: false, ritual: false, casting: null, sourceName: "", ready: true, grantedFrom,
});

const block: ClassSpellcasting = {
  classSlotId: "caster-other-spells", className: "Other Spells", ability: "" as ClassSpellcasting["ability"], saveDc: 0, attackBonus: 0,
  attackRoll: 0, model: "known", cantripsMax: 0, cantripsKnown: 1, preparedMax: 0, preparedCount: 0, maxSpellLevel: 0,
  pact: null, other: true,
  spells: [spell("ID_T_LIGHTS", "Dancing Lights", 0, "Gloom Elf"), spell("ID_T_FIRE", "Faerie Fire", 1, "Gloom Elf")],
};

const play = {
  spellSlots: [{ level: 1, max: 2, used: 0 }], pactSlots: null, concentration: null, concentrationSpells: [],
} as unknown as PlayState;

describe("Play: Other Spells", () => {
  it("lists the spells with their source, no attack or DC row and no Cast button", () => {
    render(<CasterCard c={block} play={play} selectedId={null} onSelect={() => undefined} />);
    expect(screen.getByText("Other Spells")).toBeTruthy();
    expect(screen.queryByText("Other Spells spells")).toBeNull();
    expect(screen.getByRole("button", { name: "Faerie Fire, description" })).toBeTruthy();
    expect(screen.getAllByText("(Gloom Elf)")).toHaveLength(2);
    expect(screen.queryByText(/Save DC/)).toBeNull();
    expect(screen.queryByRole("button", { name: /^Cast Faerie Fire/ })).toBeNull();
  });
});
