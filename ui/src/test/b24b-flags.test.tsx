import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

describe("Sources: real flags from the contract", () => {
  it("shows Incomplete and information badges only for flagged sources", async () => {
    const api = new MockApi();
    const real = await api.listSources();
    const [a, b] = real;
    const sources = real.map((s) =>
      s === a ? { ...s, incomplete: true } : s === b ? { ...s, info: "Errata pending." } : s);
    api.listSources = async () => sources;
    renderApp("/content/sources", { api });
    const grid = await screen.findByRole("grid", { name: "Sources" });
    await waitFor(() => expect(within(grid).getAllByRole("img", { name: "Incomplete source" })).toHaveLength(1));
    expect(within(grid).getAllByRole("img", { name: "Errata pending." })).toHaveLength(1);
  });
});

describe("Magic: prepared spell restricted by source", () => {
  it("explains a prepared spell whose source is no longer allowed", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    const snap = await api.getSnapshot({ characterId: id });
    const cls = snap.spellcasting.classes.find((c) => c.className === "Wizard")!;
    const spell = cls.spells.find((s) => s.level > 0 && !s.alwaysPrepared && !s.prepared)!;
    await api.setSpellPrepared({ characterId: id, classSlotId: cls.classSlotId, spellId: spell.id, prepared: true });
    const load = api.loadCharacter.bind(api);
    api.loadCharacter = async (a) => {
      const snap = await load(a);
      return {
        ...snap,
        spellcasting: {
          ...snap.spellcasting,
          classes: snap.spellcasting.classes.map((c) => ({
            ...c, spells: c.spells.map((x) => (x.id === spell.id ? { ...x, restrictedBySource: true } : x)),
          })),
        },
      };
    };
    renderApp("/spells", { api, openCharacterId: id });
    await user.click(await screen.findByRole("tab", { name: /wizard/i }));
    await user.click(await screen.findByRole("button", { name: spell.name }));
    expect(await screen.findByText(`${spell.name} is prepared but is restricted by source. Prepare another spell.`)).toBeTruthy();
  });
});
