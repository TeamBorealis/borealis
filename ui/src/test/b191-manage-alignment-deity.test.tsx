import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp, seededId } from "./utils";

/* B-191: Manage > Character Alignment and Deity are the character's Alignment and Deity selections, as the classic builder's
   pickers are. They list the elements those slots offer (snapshot.detailChoices); the backend selects the element a value names
   and keeps any other deity text as custom text. */

const open = async (deities?: string[]) => {
  const api = new MockApi({ rng: () => 0.5, deities });
  const id = await seededId(api, "Brannoc");
  const utils = renderApp("/manage", { api, openCharacterId: id });
  await screen.findByLabelText("Character name");
  return { ...utils, api, id };
};

const deityOptions = async (user: ReturnType<typeof userEvent.setup>) => {
  await user.click(screen.getByRole("button", { name: "Show Deity (optional) options" }));
  const names = within(screen.getByRole("listbox", { name: "Deity (optional)" })).getAllByRole("option").map((o) => o.textContent);
  await user.keyboard("{Escape}");
  return names;
};

describe("Manage alignment and deity are the Alignment and Deity selections (B-191)", () => {
  it("lists the Alignment slot's elements and sends the picked one", async () => {
    const user = userEvent.setup();
    const { api, id } = await open();
    const select = screen.getByLabelText("Alignment") as HTMLSelectElement;
    const listed = Array.from(select.options).map((o) => o.value);
    expect(listed).toEqual(["", ...(await api.getSnapshot({ characterId: id })).detailChoices.alignment]);
    expect(listed).toContain("Neutral");
    expect(listed).not.toContain("True Neutral");
    await user.selectOptions(select, "Lawful Good");
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).details.alignment).toBe("Lawful Good"));
  });

  it("shows and keeps an alignment from an older file that names no element", async () => {
    const api = new MockApi({ rng: () => 0.5 });
    const id = await seededId(api, "Brannoc");
    await api.updateDetails({ characterId: id, patch: { alignment: "True Neutral" } });
    renderApp("/manage", { api, openCharacterId: id });
    const select = (await screen.findByLabelText("Alignment")) as HTMLSelectElement;
    expect(select.value).toBe("True Neutral");
    expect(Array.from(select.options).map((o) => o.value)).toContain("True Neutral");
  });

  it("lists the content's deities, sends a pick, and keeps typed custom text", async () => {
    const user = userEvent.setup();
    const { api, id } = await open(["Hearth Mother", "Old Oak"]);
    expect(await deityOptions(user)).toEqual(["Hearth Mother", "Old Oak"]);
    await user.click(screen.getByRole("button", { name: "Show Deity (optional) options" }));
    await user.click(screen.getByRole("option", { name: "Old Oak" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).details.deity).toBe("Old Oak"));
    const deity = screen.getByRole("combobox", { name: "Deity (optional)" });
    await user.clear(deity);
    await user.type(deity, "The Lantern Keeper");
    await user.tab();
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).details.deity).toBe("The Lantern Keeper"));
  });

  it("suggests the generic deity list when the content has no deities", async () => {
    const user = userEvent.setup();
    await open();
    const names = await deityOptions(user);
    expect(names[0]).toBe("Apollo");
    expect(names).toHaveLength(14);
  });
});
