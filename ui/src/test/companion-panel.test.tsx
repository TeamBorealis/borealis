import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { CompanionState } from "../api/contract";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

const base: CompanionState = {
  creature: { id: "ID_WOLF", name: "Wolf", type: "companion", source: "Test" }, name: "Pip", portrait: null,
  armorClass: 12, hp: 9, armorClassNote: null, hpNote: null, challenge: "1/8", speed: 40,
  abilities: { str: 14, dex: 12, con: 10, int: 2, wis: 10, cha: 6 },
  proficiency: 2, initiative: 1, build: "Medium beast, unaligned", flySpeed: 0, climbSpeed: 30, swimSpeed: 0, burrowSpeed: 0,
  skills: [{ name: "Perception", ability: "WIS", bonus: 2 }], passivePerception: 12,
  traits: [{ name: "Keen Hearing", text: "Advantage on hearing checks." }], actions: [{ name: "Bite", text: "Melee attack." }], reactions: [],
};

async function open(companion: CompanionState) {
  const api = new MockApi();
  const id = await seededId(api, "Seraphine");
  const load = api.loadCharacter.bind(api);
  vi.spyOn(api, "loadCharacter").mockImplementation(async (a) => ({ ...(await load(a)), companion }));
  const user = userEvent.setup();
  renderApp("/manage", { api, openCharacterId: id });
  return user;
}

describe("the Manage > Companion tab", () => {
  it("holds the header, the sentence, Companion Name and Companions Gallery, and no statistics or CR", async () => {
    const user = await open(base);
    await user.click(await screen.findByRole("tab", { name: "Companion" }));
    const section = await screen.findByRole("region", { name: "COMPANION" });
    expect(within(section).getByText("A companion that accompanies you on your adventures and is trained to fight alongside you.")).toBeTruthy();
    expect(within(section).getByLabelText("Companion Name")).toHaveProperty("value", "Pip");
    expect(within(section).getByRole("button", { name: "Companions Gallery" }).classList.contains("mg-tool")).toBe(true);
    expect(within(section).queryByText("CR")).toBeNull();
    expect(within(section).queryByText("AC")).toBeNull();
  });

  it("has no Companion tab for a character without a companion", async () => {
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/manage", { api, openCharacterId: id });
    await screen.findByRole("tab", { name: "Character" });
    expect(screen.queryByRole("tab", { name: "Companion" })).toBeNull();
  });
});

describe("the information slider, companion context", () => {
  async function slider(companion: CompanionState) {
    const user = await open(companion);
    await user.click(await screen.findByRole("button", { name: "Seraphine Vale" }));
    await user.click(await screen.findByRole("button", { name: "SWITCH TO COMPANION" }));
    return { user, panel: within(await screen.findByLabelText("Companion")) };
  }

  it("prints name, build, the headline strip in Aurora's order and the scores with modifiers", async () => {
    const { panel } = await slider(base);
    expect(screen.getByText("WOLF")).toBeTruthy();
    expect(panel.getByText("Medium beast, unaligned")).toBeTruthy();
    const strip = panel.getByText("Proficiency:").parentElement;
    expect(strip?.textContent).toBe("Proficiency: +2Initiative: +1AC: 12Speed: 40ftHP: 9");
    expect(panel.getByText("14 (+2)")).toBeTruthy();
    expect(panel.getByText("6 (-2)")).toBeTruthy();
    expect(panel.queryByText(/CR|challenge/i)).toBeNull();
  });

  it("shows AC and HP text in the slot when the stat block gives a formula (IM12a)", async () => {
    const { panel } = await slider({ ...base, armorClass: 0, hp: 0, armorClassNote: "11 + the level of the spell", hpNote: "40 + 10 for each spell level above 4th" });
    const strip = panel.getByText("Proficiency:").parentElement;
    expect(strip?.textContent).toContain("AC: 11 + the level of the spell");
    expect(strip?.textContent).toContain("HP: 40 + 10 for each spell level above 4th");
  });

  it("opens every section by default (orchestrator default; Aurora's is unknown)", async () => {
    const { panel } = await slider(base);
    expect(panel.getByText("Passive Perception")).toBeTruthy();
    expect(panel.getByText("Keen Hearing.")).toBeTruthy();
    expect(panel.getByText("Bite.")).toBeTruthy();
    expect(panel.getAllByText("n/a").length).toBeGreaterThan(0);
  });

  it("lists the sections and the speed rows that exist, and switches back to the character", async () => {
    const { user, panel } = await slider(base);
    for (const t of ["Skills", "Traits", "Actions", "Reactions", "Additional Statistics & Breakdown"]) expect(panel.getByText(t)).toBeTruthy();
    expect(panel.getByText("Climb Speed")).toBeTruthy();
    expect(panel.queryByText("Fly Speed")).toBeNull();
    expect((screen.getByRole("button", { name: "SWITCH TO COMPANION" }) as HTMLButtonElement).disabled).toBe(true);
    await user.click(screen.getByRole("button", { name: "SWITCH TO CHARACTER" }));
    expect(screen.queryByLabelText("Companion")).toBeNull();
  });
});
