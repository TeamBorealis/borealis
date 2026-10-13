import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { triState } from "../components/controls";
import { renderApp, seededId } from "./utils";

describe("sheet auto-generate", () => {
  it("shows a generated preview on open when autoGenerate is on (the default)", async () => {
    const api = new MockApi({ rng: () => 0.5 });
    const id = await seededId(api, "Brannoc");
    renderApp("/sheet", { api, openCharacterId: id });
    expect(await screen.findByTitle("Character sheet preview")).toBeTruthy();
    expect(screen.queryByText(/No preview yet/)).toBeNull();
  });

  it("waits for Generate Preview when autoGenerate is off", async () => {
    const api = new MockApi({ rng: () => 0.5 });
    const id = await seededId(api, "Brannoc");
    const cur = await api.getSettings();
    await api.saveSettings({ patch: { sheet: { ...cur.sheet, autoGenerate: false } } });
    renderApp("/sheet", { api, openCharacterId: id });
    expect(await screen.findByText(/No preview yet/)).toBeTruthy();
    expect(screen.queryByTitle("Character sheet preview")).toBeNull();
  });
});

describe("sources tri-state", () => {
  it("derives checked / mixed from member counts", () => {
    expect(triState(0, 3)).toEqual({ checked: false, mixed: false });
    expect(triState(2, 3)).toEqual({ checked: false, mixed: true });
    expect(triState(3, 3)).toEqual({ checked: true, mixed: false });
    expect(triState(0, 0)).toEqual({ checked: false, mixed: false });
  });

  it("a group checkbox toggles its sources and shows the mixed state when only some are allowed", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ rng: () => 0.5 });
    const id = await seededId(api, "Brannoc");
    renderApp("/content/sources", { api, openCharacterId: id });
    const grid = await screen.findByRole("grid", { name: "Sources" });
    const groups = await within(grid).findAllByRole("rowgroup");
    const first = groups[0]!;
    const groupBox = within(first).getAllByRole("checkbox")[0] as HTMLInputElement;
    expect(groupBox.checked).toBe(true);
    await user.click(groupBox);
    const apply = await screen.findByRole("button", { name: "Apply" }, { timeout: 500 }).catch(() => null);
    if (apply) await user.click(apply);
    await waitFor(() => expect((within(first).getAllByRole("checkbox")[0] as HTMLInputElement).checked).toBe(false));
    const members = within(first).getAllByRole("checkbox").slice(1) as HTMLInputElement[];
    expect(members.every((m) => !m.checked)).toBe(true);
    await user.click(members[0]!);
    await waitFor(() => expect((within(first).getAllByRole("checkbox")[0] as HTMLInputElement).indeterminate || members.length === 1).toBe(true));
  });

  it("offers the playtest and compendium actions and no bulk buttons", async () => {
    renderApp("/content/sources");
    expect(await screen.findByRole("button", { name: "Restrict all playtest material" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "View selected source in compendium" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Allow all listed sources" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Restore every source" })).toBeNull();
  });
});
