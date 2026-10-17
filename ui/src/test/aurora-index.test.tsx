import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { ContentIndex, UpdateIndexesResult } from "../api/contract";
import { sourceConsentText } from "../pages/content/IndexesTab";
import { describeIndexUpdate, UP_TO_DATE } from "../pages/content/indexUpdateText";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

const URL = "https://raw.githubusercontent.com/aurorabuilder/elements/master/core.index";

describe("adding an index file asks for consent first", () => {
  it("Cancel adds nothing; Add source adds the index, shown as unverified", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/content");
    const input = await screen.findByLabelText("Index URL");
    await user.type(input, URL);
    await user.click(screen.getByRole("button", { name: "Fetch" }));
    let dlg = await screen.findByRole("dialog", { name: "Add content source" });
    expect(within(dlg).getByText(sourceConsentText("raw.githubusercontent.com"))).toBeTruthy();
    await user.click(within(dlg).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Add content source" })).toBeNull());
    expect(await api.listIndexes()).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Fetch" }));
    dlg = await screen.findByRole("dialog", { name: "Add content source" });
    await user.click(within(dlg).getByRole("button", { name: "Add source" }));
    const list = await screen.findByRole("list", { name: "Indexes" });
    expect(within(list).getByText("unverified")).toBeTruthy();
    expect(await api.listIndexes()).toHaveLength(1);
  });

  it("a per-index problem is shown in the list", async () => {
    const api = new MockApi({ rng: () => 0.5 });
    const added = (await api.addIndex({ url: URL }))[0]!;
    // The mock backend never fails; give it the reason the real backend reports.
    Object.assign(added, { problem: "the server answered 404 Not Found for 3 files" });
    renderApp("/content", { api });
    expect(await screen.findByText("the server answered 404 Not Found for 3 files")).toBeTruthy();
  });
});

const result = (over: Partial<UpdateIndexesResult> = {}): UpdateIndexesResult =>
  ({ updated: 0, failed: [], reloaded: false, changed: [], newHosts: [], reports: [], ...over });
const record = (over: Partial<ContentIndex> = {}): ContentIndex =>
  ({ url: URL, name: "Core", lastChecked: "2026-10-15T10:00:00Z", fileCount: 238, updatesAvailable: 0, requiresNewerApp: false, verified: false, problem: null, ...over });
const CHECKED = "Last checked at 10:00";

describe("update wording never calls a failure up to date (index-bug-1)", () => {
  it("a failed index with nothing installed is an error that names the reason", () => {
    const t = describeIndexUpdate(result({ failed: [URL] }), [record({ lastChecked: null, problem: "this address is a web page, not an index file" })], CHECKED);
    expect(t.tone).toBe("error");
    expect(t.toast).toContain("Nothing was downloaded");
    expect(t.toast).toContain("Core: this address is a web page, not an index file");
    expect(`${t.toast} ${t.status}`.toLowerCase()).not.toContain("up to date");
  });

  it("a partial update is a warning that counts both", () => {
    const t = describeIndexUpdate(result({ updated: 1, failed: ["https://x.invalid/a.xml"] }), [record({ problem: "the server answered 404 Not Found (https://x.invalid/a.xml)" })], CHECKED);
    expect(t.tone).toBe("warning");
    expect(t.toast).toContain("1 content pack(s) downloaded and loaded; 1 could not be updated.");
    expect(t.toast.toLowerCase()).not.toContain("up to date");
  });

  it("an index that was never downloaded is not up to date even without failures", () => {
    const t = describeIndexUpdate(result(), [record({ lastChecked: null })], CHECKED);
    expect(t.tone).toBe("warning");
    expect(t.toast).toBe("Nothing was downloaded: Core has not been downloaded yet.");
  });

  it("an installed update and a clean check read as such", () => {
    expect(describeIndexUpdate(result({ updated: 1, reloaded: true }), [record()], CHECKED).toast).toBe("1 content pack(s) downloaded and loaded.");
    const clean = describeIndexUpdate(result(), [record()], CHECKED);
    expect([clean.toast, clean.status, clean.tone]).toEqual([UP_TO_DATE, CHECKED, "info"]);
  });
});

describe("the Indexes tab (index-bug-1)", () => {
  it("Fetch adds the index and downloads it right away", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/content");
    const spy = vi.spyOn(api, "updateIndexes");
    await user.type(await screen.findByLabelText("Index URL"), URL);
    await user.click(screen.getByRole("button", { name: "Fetch" }));
    const dlg = await screen.findByRole("dialog", { name: "Add content source" });
    await user.click(within(dlg).getByRole("button", { name: "Add source" }));
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getAllByText("3 content pack(s) downloaded and loaded.").length).toBeGreaterThan(0));
    expect((await api.listIndexes())[0]!.lastChecked).not.toBeNull();
  });

  it("Update with a failed index shows the reason, never 'Everything is up to date.'", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ rng: () => 0.5 });
    await api.addIndex({ url: URL });
    vi.spyOn(api, "updateIndexes").mockResolvedValue(result({ failed: [URL] }));
    vi.spyOn(api, "listIndexes").mockResolvedValue([record({ name: "core", lastChecked: null, problem: "this address is a web page, not an index file" })]);
    renderApp("/content", { api });
    await user.click(await screen.findByRole("button", { name: "Update" }));
    await waitFor(() => expect(screen.getAllByText(/Nothing was downloaded: 1 could not be updated\./).length).toBeGreaterThan(0));
    expect(screen.queryByText(UP_TO_DATE)).toBeNull();
    expect(screen.getAllByText(/this address is a web page, not an index file/).length).toBeGreaterThan(0);
  });

  it("an index that has not been downloaded says so in the list", async () => {
    const api = new MockApi({ rng: () => 0.5 });
    await api.addIndex({ url: URL });
    renderApp("/content", { api });
    expect(await screen.findByText("Not downloaded yet. Press Update to download it.")).toBeTruthy();
  });
});
