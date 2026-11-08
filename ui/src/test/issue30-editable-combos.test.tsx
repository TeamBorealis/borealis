import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

/* Issue #30: every Manage > Character detail combo takes a value from its list or typed text, as the
   classic builder's editable combos do (gender, eyes, skin, hair, trinket). Alignment stays a closed list. */

const open = async (route = "/manage") => {
  const api = new MockApi({ rng: () => 0.5 });
  const id = await seededId(api, "Brannoc");
  const utils = renderApp(route, { api, openCharacterId: id });
  return { ...utils, api, id };
};

const optionsOf = async (user: ReturnType<typeof userEvent.setup>, label: string) => {
  await user.click(screen.getByRole("button", { name: `Show ${label} options` }));
  const names = within(screen.getByRole("listbox", { name: label })).getAllByRole("option").map((o) => o.textContent);
  await user.keyboard("{Escape}");
  return names;
};

describe("Manage character details: editable combos (issue #30)", () => {
  it("makes gender, deity, eyes, skin and hair editable combos and keeps alignment a closed list", async () => {
    await open();
    await screen.findByLabelText("Character name");
    for (const label of ["Gender", "Deity (optional)", "Eyes", "Skin", "Hair"]) {
      const box = screen.getByRole("combobox", { name: label });
      expect(box.tagName).toBe("INPUT");
    }
    expect(screen.getByLabelText("Alignment").tagName).toBe("SELECT");
  });

  it("suggests the classic builder's gender, eyes, skin and hair lists", async () => {
    const user = userEvent.setup();
    await open();
    await screen.findByLabelText("Character name");
    expect(await optionsOf(user, "Gender")).toEqual(["Male", "Female", "Non-binary", "Construct"]);
    expect(await optionsOf(user, "Eyes")).toEqual(["Blue", "Brown", "Red", "Green", "Black", "White", "Purple", "Orange", "Pink", "None", "Eye Patch", "Glass Eye"]);
    expect(await optionsOf(user, "Skin")).toEqual(["Pale", "Fair", "White", "Brown", "Black", "Albino", "Obsidian", "Red Scales", "Brass Scales", "Green Scales", "Black Scales", "Furry", "Barkskin"]);
    expect(await optionsOf(user, "Hair")).toEqual(["Bald", "Short", "Long", "Brown", "Red", "White", "Gray", "Black", "Purple", "Green"]);
  });

  it("stores typed text and picked options, and shows them again after save and reopen", async () => {
    const user = userEvent.setup();
    const { api, id, unmount } = await open();
    const gender = await screen.findByRole("combobox", { name: "Gender" });
    await user.clear(gender);
    await user.type(gender, "Genderfluid");
    await user.tab();
    const eyes = screen.getByRole("combobox", { name: "Eyes" });
    await user.clear(eyes);
    await user.type(eyes, "Heterochromia{Enter}");
    const deity = screen.getByRole("combobox", { name: "Deity (optional)" });
    await user.clear(deity);
    await user.type(deity, "The Lantern Keeper");
    await user.tab();
    await user.click(screen.getByRole("button", { name: "Show Hair options" }));
    await user.click(screen.getByRole("option", { name: "Long" }));
    await waitFor(async () => {
      const d = (await api.getSnapshot({ characterId: id })).details;
      expect([d.gender, d.eyes, d.deity, d.hair]).toEqual(["Genderfluid", "Heterochromia", "The Lantern Keeper", "Long"]);
    });
    await api.saveCharacter({ characterId: id });
    await api.closeCharacter({ characterId: id, discard: false });
    unmount();
    const back = await api.loadCharacter({ characterId: id });
    expect([back.details.gender, back.details.eyes, back.details.deity, back.details.hair]).toEqual(["Genderfluid", "Heterochromia", "The Lantern Keeper", "Long"]);
    renderApp("/manage", { api, openCharacterId: id });
    expect(((await screen.findByRole("combobox", { name: "Gender" })) as HTMLInputElement).value).toBe("Genderfluid");
    expect((screen.getByRole("combobox", { name: "Eyes" }) as HTMLInputElement).value).toBe("Heterochromia");
  });

  it("offers Non-binary for gender, as New Character does, and stores it when picked (decision FU1)", async () => {
    const user = userEvent.setup();
    const { api, id } = await open();
    await screen.findByLabelText("Character name");
    await user.click(screen.getByRole("button", { name: "Show Gender options" }));
    await user.click(within(screen.getByRole("listbox", { name: "Gender" })).getByRole("option", { name: "Non-binary" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).details.gender).toBe("Non-binary"));
    expect((screen.getByRole("combobox", { name: "Gender" }) as HTMLInputElement).value).toBe("Non-binary");
  });

  it("makes the backstory trinket an editable combo over the trinket list", async () => {
    const user = userEvent.setup();
    const { api, id } = await open();
    await user.click(await screen.findByRole("tab", { name: "Backstory" }));
    const trinket = await screen.findByRole("combobox", { name: "Trinket" });
    await user.clear(trinket);
    await user.type(trinket, "A button from my father's coat");
    await user.tab();
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).details.trinket).toBe("A button from my father's coat"));
    await user.click(screen.getByRole("button", { name: "Show Trinket options" }));
    await user.click(screen.getByRole("option", { name: "A key that fits no known lock" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).details.trinket).toBe("A key that fits no known lock"));
  });
});
