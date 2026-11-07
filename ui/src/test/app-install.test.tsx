import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import type { AppUpdateStatus } from "../api/types";
import { apiError } from "../api/mock/errors";
import { checkWithFetch, resetAppUpdate } from "../state/appUpdate";
import { openSettingsTab, renderApp, seededId } from "./utils";

/** Install and restart (#25, UP2): the notice's button states for each way a copy can update. */
const NEWER: AppUpdateStatus = {
  state: "done", current: "1.0.0", latest: "1.0.1", newer: true, url: "https://borealisbuilder.com/#download",
  releaseUrl: null, notes: null, checkedAt: "2026-11-07T09:00:00Z", updateMethod: "install",
};
const fast = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });

beforeEach(() => { resetAppUpdate(); });

async function openFlyout(api: MockApi) {
  const user = userEvent.setup();
  renderApp("/characters", { api });
  await user.click(await screen.findByRole("button", { name: "Update Available" }));
  const flyout = await screen.findByRole("dialog", { name: "Update Available" });
  return { user, flyout };
}

/** A promise the test resolves by hand, to hold the mock install in its busy state. */
function gate() {
  let open = () => undefined as void;
  const hold = new Promise<void>((resolve) => { open = resolve; });
  return { hold, open };
}

describe("UPDATE AVAILABLE flyout: Install and restart", () => {
  it("a copy that can install offers Install and restart next to Download and Close", async () => {
    const api = fast();
    api.appUpdate.status = { ...NEWER };
    const { flyout } = await openFlyout(api);
    const names = within(flyout).getAllByRole("button").map((b) => b.textContent);
    expect(names).toEqual(expect.arrayContaining(["Install and restart", "Download", "Close"]));
    expect(names.indexOf("Install and restart")).toBe(names.indexOf("Download") - 1);
    const install = within(flyout).getByRole("button", { name: "Install and restart" });
    expect(install.className).toContain("btn-success");
    expect(within(flyout).getByRole("button", { name: "Download" }).className).not.toContain("btn-success");
    expect(flyout.textContent).toContain("checks its signature");
  });

  it("a click installs only then: busy with download progress, then restarting", async () => {
    const api = fast();
    api.appUpdate.status = { ...NEWER };
    api.appUpdate.installProgress = [{ downloaded: 0, total: 200 }, { downloaded: 84, total: 200 }];
    const { hold, open } = gate();
    api.appUpdate.hold = hold;
    const { user, flyout } = await openFlyout(api);
    expect(api.appUpdate.installs).toBe(0);
    await user.click(within(flyout).getByRole("button", { name: "Install and restart" }));
    const busy = await within(flyout).findByRole("button", { name: "Installing... 42%" });
    expect((busy as HTMLButtonElement).disabled).toBe(true);
    await user.click(busy);
    expect(api.appUpdate.installs).toBe(1);
    // Download stays available while it runs.
    expect((within(flyout).getByRole("button", { name: "Download" }) as HTMLButtonElement).disabled).toBe(false);
    await act(async () => { open(); });
    const restarting = await within(flyout).findByRole("button", { name: "Restarting..." });
    expect((restarting as HTMLButtonElement).disabled).toBe(true);
  });

  it("without a known size it says Installing... and a failure shows why and allows another try", async () => {
    const api = fast();
    api.appUpdate.status = { ...NEWER };
    api.appUpdate.installError = apiError("requirement_failed", "The download did not pass the signature check, so nothing was installed. Use Download instead.");
    const { hold, open } = gate();
    api.appUpdate.hold = hold;
    const { user, flyout } = await openFlyout(api);
    await user.click(within(flyout).getByRole("button", { name: "Install and restart" }));
    expect(await within(flyout).findByRole("button", { name: "Installing..." })).toBeTruthy();
    await act(async () => { open(); });
    expect((await within(flyout).findByRole("alert")).textContent).toContain("did not pass the signature check");
    const again = within(flyout).getByRole("button", { name: "Install and restart" }) as HTMLButtonElement;
    expect(again.disabled).toBe(false);
    api.appUpdate.installError = null;
    api.appUpdate.hold = null;
    await user.click(again);
    await waitFor(() => expect(api.appUpdate.installs).toBe(2));
    await waitFor(() => expect(within(flyout).queryByRole("alert")).toBeNull());
  });

  it("a download-only copy (store rules, no key yet, unpackaged) shows no install button", async () => {
    const api = fast();
    api.appUpdate.status = { ...NEWER, updateMethod: "download" };
    const { flyout } = await openFlyout(api);
    expect(within(flyout).queryByRole("button", { name: "Install and restart" })).toBeNull();
    expect(within(flyout).getByRole("button", { name: "Download" }).className).toContain("btn-success");
    expect(flyout.textContent).toContain("Download it from the Borealis website.");
  });

  it("a deb or rpm install keeps the notice and points to the package manager", async () => {
    const api = fast();
    api.appUpdate.status = { ...NEWER, updateMethod: "package-manager" };
    const { flyout } = await openFlyout(api);
    expect(within(flyout).queryByRole("button", { name: "Install and restart" })).toBeNull();
    expect(flyout.textContent).toContain("update it the way you installed it");
    expect(within(flyout).getByRole("button", { name: "Download" })).toBeTruthy();
  });

  it("unsaved changes to the open character are saved before the restart", async () => {
    const api = fast();
    api.presetSettings({ autosave: false });
    const copy = await api.duplicateCharacter({ characterId: await seededId(api, "Seraphine") });
    expect((await api.getSnapshot({ characterId: copy.id })).dirty).toBe(true);
    api.appUpdate.status = { ...NEWER };
    const save = vi.spyOn(api, "saveCharacter");
    const install = vi.spyOn(api, "installAppUpdate");
    const user = userEvent.setup();
    renderApp("/characters", { api, openCharacterId: copy.id });
    await user.click(await screen.findByRole("button", { name: "Update Available" }));
    const flyout = await screen.findByRole("dialog", { name: "Update Available" });
    await user.click(within(flyout).getByRole("button", { name: "Install and restart" }));
    await waitFor(() => expect(install).toHaveBeenCalledTimes(1));
    expect(save).toHaveBeenCalledWith({ characterId: copy.id });
    expect(save.mock.invocationCallOrder[0]!).toBeLessThan(install.mock.invocationCallOrder[0]!);
  });
});

describe("Settings > Updates: Install and restart", () => {
  it("appears with Download when the copy can install, and reports its progress", async () => {
    const user = userEvent.setup();
    const api = fast();
    api.appUpdate.installProgress = [{ downloaded: 50, total: 100 }];
    const { hold, open } = gate();
    api.appUpdate.hold = hold;
    renderApp("/characters", { api });
    await openSettingsTab(user, "Updates");
    expect(screen.queryByRole("button", { name: "Install and restart" })).toBeNull();
    api.appUpdate.status = { ...NEWER };
    await user.click(screen.getByRole("button", { name: "Check now" }));
    await user.click(await screen.findByRole("button", { name: "Install and restart" }));
    expect(await screen.findByText("Downloading the update: 50%.")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Installing... 50%" }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByRole("button", { name: "Download" })).toBeTruthy();
    await act(async () => { open(); });
    expect(await screen.findByText("The update is installed. Borealis is restarting.")).toBeTruthy();
  });

  it("a package-manager copy gets no install button and is told how to update", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/characters", { api });
    await openSettingsTab(user, "Updates");
    api.appUpdate.status = { ...NEWER, updateMethod: "package-manager" };
    await user.click(screen.getByRole("button", { name: "Check now" }));
    expect(await screen.findByText(/Update it the way you installed it, or download the new package\./)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Install and restart" })).toBeNull();
  });
});

describe("phones", () => {
  it("the fetch() check never offers an install", async () => {
    const answer = vi.fn(async () => ({ ok: true, json: async () => ({ tag_name: "v9.0.0" }) }) as Response) as unknown as typeof fetch;
    expect((await checkWithFetch("1.0.0", answer)).updateMethod).toBe("download");
  });
});
