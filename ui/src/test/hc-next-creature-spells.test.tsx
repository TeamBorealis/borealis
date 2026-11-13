import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import type { HomebrewStatSpellcasting } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { CreatorEnvValue } from "../pages/homebrew/env";
import { SpellcastingPart } from "../pages/homebrew/creature/SpellcastingPart";
import { ApiProvider } from "../state/ApiContext";

// A creature's stat block spell list (board HwTd4): a spell that isn't loaded is added by its name, and each spell's
// note ("level 5 version") is edited in place.

let last: HomebrewStatSpellcasting | null = null;
function Harness({ start }: { start: HomebrewStatSpellcasting }) {
  const [value, setValue] = useState(start);
  return (
    <ApiProvider api={new MockApi()}>
      <CreatorEnvValue value={{}}>
        <SpellcastingPart value={value} onChange={(v) => { last = v; setValue(v); }} />
      </CreatorEnvValue>
    </ApiProvider>
  );
}

const START: HomebrewStatSpellcasting = { ability: "wis", lists: [{ frequency: "at_will", spells: [{ name: "Fire Bolt", id: "ID_SRD_SPELL_FIRE_BOLT" }] }] };

describe("creature spell lists", () => {
  it("adds a spell that isn't loaded by its name, once", async () => {
    const user = userEvent.setup();
    render(<Harness start={START} />);
    const box = screen.getByRole("textbox", { name: "Spell list 1: a spell by name" });
    const add = screen.getByRole("button", { name: "Spell list 1: add the spell by name" });
    expect((add as HTMLButtonElement).disabled).toBe(true);
    await user.type(box, "  Ember Ward ");
    await user.click(add);
    expect(last?.lists[0]?.spells).toEqual([{ name: "Fire Bolt", id: "ID_SRD_SPELL_FIRE_BOLT" }, { name: "Ember Ward" }]);
    expect((box as HTMLInputElement).value).toBe("");
    await user.type(box, "fire bolt{Enter}");
    expect(last?.lists[0]?.spells).toHaveLength(2);
  });

  it("edits a spell's note in place", async () => {
    const user = userEvent.setup();
    render(<Harness start={START} />);
    await user.type(screen.getByRole("textbox", { name: "Fire Bolt: note" }), "level 5 version");
    expect(last?.lists[0]?.spells[0]).toEqual({ name: "Fire Bolt", id: "ID_SRD_SPELL_FIRE_BOLT", note: "level 5 version" });
    await user.clear(screen.getByRole("textbox", { name: "Fire Bolt: note" }));
    expect(last?.lists[0]?.spells[0]).toEqual({ name: "Fire Bolt", id: "ID_SRD_SPELL_FIRE_BOLT" });
  });
});
