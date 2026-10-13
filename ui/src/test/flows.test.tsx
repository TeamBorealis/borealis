import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

describe("create character flow", () => {
  it("creates a character from the dialog and lands in the builder", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters");
    await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
    const dialog = await screen.findByRole("dialog", { name: "New character" });
    await user.type(within(dialog).getByLabelText("Name"), "Test Hero");
    await user.click(within(dialog).getByRole("button", { name: "Create character" }));
    expect(await screen.findByRole("heading", { name: "Build" })).toBeTruthy();
    const list = await api.listCharacters();
    expect(list.some((c) => c.name === "Test Hero")).toBe(true);
    // the shell shows the open character in the menu bar, profile block and status bar
    expect((await screen.findAllByText("Test Hero")).length).toBeGreaterThan(0);
  });
});

describe("selection picker", () => {
  it("picks a class from the class list and shows it in the footer", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const snap = await api.createCharacter({ request: { name: "Picker", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
    renderApp("/builder/class", { api, openCharacterId: snap.id });
    const list = await screen.findByRole("list", { name: "Class options" });
    expect(within(list).queryByRole("button", { name: "Select" })).toBeNull();
    const fighter = await within(list).findByRole("button", { name: /Fighter/ });
    await user.click(fighter);
    expect((await api.getSnapshot({ characterId: snap.id })).slots.some((s) => s.chosen?.id === "fighter")).toBe(false); // click only describes
    await user.dblClick(fighter);
    expect((await screen.findAllByRole("button", { name: "Clear Fighter" })).length).toBeGreaterThan(0);
    const after = await api.getSnapshot({ characterId: snap.id });
    expect(after.slots.some((s) => s.chosen?.id === "fighter")).toBe(true);
    expect(after.slots.some((s) => s.title === "Fighting style")).toBe(true);
  });
});

describe("point buy budget", () => {
  it("rejects overspending at the API and disables the stepper in the UI", async () => {
    const api = new MockApi();
    const snap = await api.createCharacter({ request: { name: "Budget", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
    // 15 costs 9; three 15s cost 27 (the whole budget); a fourth must be refused.
    for (const k of ["str", "dex", "con"] as const) await api.setBaseScore({ characterId: snap.id, ability: k, value: 15 });
    await expect(api.setBaseScore({ characterId: snap.id, ability: "int", value: 9 })).rejects.toMatchObject({ code: "budget_exceeded" });
    await expect(api.setBaseScore({ characterId: snap.id, ability: "wis", value: 15 })).rejects.toMatchObject({ code: "budget_exceeded" });
    const s = await api.getSnapshot({ characterId: snap.id });
    expect(s.abilities.pointBuy!.remaining).toBeGreaterThanOrEqual(0);

    const user = userEvent.setup();
    renderApp("/builder/abilities", { api, openCharacterId: snap.id });
    const inc = await screen.findByRole("button", { name: "Increase Wisdom" });
    expect((inc as HTMLButtonElement).disabled).toBe(true);
    await user.click(screen.getByRole("button", { name: "Decrease Strength" }));
    await waitFor(() => expect((screen.getByRole("button", { name: "Increase Wisdom" }) as HTMLButtonElement).disabled).toBe(false));
  });

  it("standard array swaps instead of duplicating values", async () => {
    const api = new MockApi();
    const snap = await api.createCharacter({ request: { name: "Array", ruleset: "dnd2014", startingLevel: 1, gender: "", abilityMethod: "standard_array", options: {} } });
    await api.assignStandardArray({ characterId: snap.id, ability: "str", value: 15 });
    const s = await api.assignStandardArray({ characterId: snap.id, ability: "dex", value: 15 });
    expect(s.abilities.standardArray!.assigned.str).toBeNull();
    expect(s.abilities.standardArray!.assigned.dex).toBe(15);
  });
});

describe("level up flyout", () => {
  it("opens from the quick bar as a right flyout and closes with Escape", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const id = await seededId(api, "Brannoc");
    renderApp("/sheet", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: "Level Up" }));
    const dlg = await screen.findByRole("dialog", { name: /Level up/i });
    expect(within(dlg).queryByLabelText("Experience points")).toBeNull();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /Level up/i })).toBeNull());
  });
});

describe("level up", () => {
  it("shows the level-up prompt when XP crosses a threshold and levels up on request", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const id = await seededId(api, "Brannoc");
    expect((await api.getSnapshot({ characterId: id })).level.levelUpAvailable).toBe(false);
    await api.setXp({ characterId: id, xp: 2700 }); // experience is edited in Manage; the flyout only prompts
    renderApp("/sheet", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: "Level Up" }));
    expect((await screen.findAllByText(/Level up available/)).length).toBeGreaterThan(0);
    expect((await api.getSnapshot({ characterId: id })).level.total).toBe(3); // never auto-levels
    await user.click(await screen.findByRole("button", { name: /Level up Fighter/ }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).level.total).toBe(4));
    expect(await screen.findByText(/^Level 4 /)).toBeTruthy();
    // the new level opens an ability score improvement pick
    const snap = await api.getSnapshot({ characterId: id });
    expect(snap.slots.some((x) => x.title === "Ability Score Improvement" && x.requiredLevel === 4 && !x.chosen)).toBe(true);
  });

  it("removing a level sets XP to the new level threshold", async () => {
    const api = new MockApi();
    const id = await seededId(api, "Brannoc");
    const s = await api.levelDown({ characterId: id });
    expect(s.level.total).toBe(2);
    expect(s.level.xp).toBe(300);
  });
});

describe("inventory", () => {
  it("adds a dagger from the catalog, equips it and offers to add it to attacks", async () => {
    const user = userEvent.setup();
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/equipment", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: /^Weapons \(/ }));
    await user.click(await screen.findByRole("button", { name: /^Dagger$/ }));
    await user.click(await screen.findByRole("button", { name: "Add" }));
    await user.click(await screen.findByRole("button", { name: "Equip Dagger" }));
    const prompt = await screen.findByRole("dialog", { name: "Add to attacks?" });
    await user.click(within(prompt).getByRole("button", { name: "Add attack" }));
    await waitFor(async () => {
      const s = await api.getSnapshot({ characterId: id });
      expect(s.inventory.items.find((i) => i.itemId === "dagger")?.equipped).toBe("primary");
      expect(s.attacks.find((a) => a.name === "Dagger")?.attackBonus).toMatch(/^\+/);
    });
  });

  it("enforces the attunement limit: the 4th item equips unattuned with a warning", async () => {
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    let s = await api.getSnapshot({ characterId: id });
    for (const itemId of ["ring-of-protection", "cloak-of-protection", "amulet-of-vigor", "wand-of-sparks"]) {
      s = await api.addItem({ characterId: id, itemId, quantity: 1, buy: false });
      const inv = s.inventory.items.find((i) => i.itemId === itemId)!;
      s = await api.equipItem({ characterId: id, inventoryId: inv.id, slot: "auto" });
    }
    expect(s.inventory.attunement.used).toBe(3);
    expect(s.inventory.warnings.length).toBe(1);
    expect(s.inventory.items.find((i) => i.itemId === "wand-of-sparks")?.attuned).toBe(false);
  });

  it("make-change keeps total value when buying with a larger coin", async () => {
    const api = new MockApi();
    const id = await seededId(api, "Seraphine");
    const before = (await api.getSnapshot({ characterId: id })).inventory.totalGp;
    const after = await api.addItem({ characterId: id, itemId: "rope", quantity: 1, buy: true });
    expect(after.inventory.totalGp).toBeCloseTo(before - 1, 5);
  });
});

const STREAM_TIMEOUT_MS = 10_000;

describe("AI assistant", () => {
  it("streams the reply into the conversation and can validate a homebrew draft", async () => {
    const user = userEvent.setup();
    renderApp("/ai", { aiEnabled: true });
    await screen.findByText(/claude ready/);
    await user.click(screen.getByRole("button", { name: "Rules Q&A" }));
    await user.type(screen.getByLabelText("Your message"), "How does grappling work?");
    await user.click(screen.getByRole("button", { name: "Send" }));
    const assistant = await waitFor(() => {
      const el = document.querySelector('.msg[data-role="assistant"]');
      expect(el?.textContent?.length ?? 0).toBeGreaterThan(5);
      return el as HTMLElement;
    }, { timeout: STREAM_TIMEOUT_MS });
    expect(assistant).toBeTruthy();
    // Stream finished: Send replaces Stop. The mock streams many timer-paced chunks, so allow for a loaded CPU.
    await screen.findByRole("button", { name: "Send" }, { timeout: STREAM_TIMEOUT_MS });
    expect(document.querySelector('.msg[data-role="assistant"]')?.textContent).toMatch(/grappled/);
  });

  it("homebrew mode shows a JSON preview that validates and can be added to a pack", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/ai", { aiEnabled: true });
    await screen.findByText(/claude ready/);
    await user.click(screen.getByRole("button", { name: "Homebrew" }));
    await user.type(screen.getByLabelText("Your message"), "a cursed item that whispers");
    await user.click(screen.getByRole("button", { name: "Send" }));
    const json = await screen.findByLabelText(/Element JSON/);
    await user.click(screen.getByRole("button", { name: "Validate" }));
    expect(await screen.findByText(/Valid against the content schema/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Add to my pack" }));
    await waitFor(async () => expect((await api.listPacks()).some((p) => p.kind === "homebrew")).toBe(true));
    expect((json as HTMLTextAreaElement).value).toContain("homebrew-");
  });
});

describe("5etools local import", () => {
  it("scans a folder, imports selected books with progress and installs non-redistributable packs", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/content");
    const progressSeen: string[] = [];
    api.subscribe("import5etools:progress", (p) => progressSeen.push(p.phase));
    // One native folder dialog, then the scan: there is no typed path field.
    expect(screen.queryByRole("textbox", { name: /Data folder/ })?.hasAttribute("readonly")).toBe(true);
    const picks: string[] = [];
    const pickPath = api.pickPath.bind(api);
    api.pickPath = async (a) => { picks.push(a.purpose); return pickPath(a); };
    await user.click(screen.getByRole("button", { name: "Scan folder…" }));
    const list = await screen.findByRole("list", { name: "Source books" });
    expect(picks).toEqual(["5etools-folder"]);
    expect((screen.getByLabelText("Data folder") as HTMLInputElement).value).toBe("/home/me/5etools/data");
    expect(within(list).getAllByText(/^20(14|24)$/).length).toBe(3);
    await user.click(within(list).getByRole("checkbox", { name: /Basic Rules/ }));
    await user.click(screen.getByRole("button", { name: "Import 2 book(s)" }));
    expect(await screen.findByText(/Partially converted/)).toBeTruthy();
    expect(progressSeen).toContain("converting");
    expect(screen.getByText(/Skipped/)).toBeTruthy();
    const packs = await api.listPacks();
    const local = packs.filter((p) => p.kind === "local-import");
    expect(local.length).toBe(2);
    expect(local.every((p) => p.redistributable === false)).toBe(true);
  });

  it("reports a missing folder and refuses raw paths or foreign tokens", async () => {
    const api = new MockApi();
    api.picks.next = "/missing";
    const [missing] = await api.pickPath({ purpose: "5etools-folder" });
    await expect(api.scan5etoolsFolder({ folder: missing!.token })).rejects.toMatchObject({ code: "not_found" });
    await expect(api.scan5etoolsFolder({ folder: "/home/me/5etools/data" })).rejects.toMatchObject({ code: "invalid_argument" });
    const [file] = await api.pickPath({ purpose: "import-file" });
    await expect(api.scan5etoolsFolder({ folder: file!.token })).rejects.toMatchObject({ code: "invalid_argument" });
  });
});
