import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { checkWithFetch, isNewer, MAX_NOTES_CHARS, openDownload, parseVersion, resetAppUpdate } from "../state/appUpdate";
import { openBorealisSettings, renderApp } from "./utils";

const NEWER = { state: "done", current: "1.0.0", latest: "1.0.1", newer: true, url: "https://borealisbuilder.com/#download", releaseUrl: null, notes: null, checkedAt: "2026-10-15T09:00:00Z" } as const;
const fast = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });

beforeEach(() => { resetAppUpdate(); });

describe("version rules", () => {
  it("compares numerically, ignores a leading v and pre-releases", () => {
    expect(isNewer("v1.0.1", "1.0.0")).toBe(true);
    expect(isNewer("1.10.0", "1.9.9")).toBe(true);
    expect(isNewer("1.0.0", "1.0.0")).toBe(false);
    expect(isNewer("v2.0.0-rc1", "1.0.0")).toBe(false);
    expect(parseVersion("nightly")).toBeNull();
    expect(parseVersion("v1.2.3+b5")).toEqual([1, 2, 3]);
  });
});

describe("phone check with fetch()", () => {
  const answer = (body: unknown, ok = true) => vi.fn(async () => ({ ok, json: async () => body }) as Response) as unknown as typeof fetch;
  it("reports a newer release and the download page", async () => {
    const s = await checkWithFetch("1.0.0", answer({ tag_name: "v1.0.1", html_url: "https://github.com/alon-mini/borealis/releases/tag/v1.0.1" }));
    expect(s).toMatchObject({ state: "done", latest: "1.0.1", newer: true, url: "https://borealisbuilder.com/#download" });
  });
  it("carries the release notes, trimmed and capped, and none when blank", async () => {
    const base = { tag_name: "v1.0.1" };
    expect((await checkWithFetch("1.0.0", answer({ ...base, body: "  ## New\r\n- a fix \n" }))).notes).toBe("## New\n- a fix");
    expect((await checkWithFetch("1.0.0", answer({ ...base, body: "  \n" }))).notes).toBeNull();
    expect((await checkWithFetch("1.0.0", answer({ ...base, body: null }))).notes).toBeNull();
    expect((await checkWithFetch("1.0.0", answer(base))).notes).toBeNull();
    const long = (await checkWithFetch("1.0.0", answer({ ...base, body: "x".repeat(MAX_NOTES_CHARS + 10) }))).notes!;
    expect(long).toHaveLength(MAX_NOTES_CHARS + 3);
    expect(long.endsWith("...")).toBe(true);
  });
  it("is quiet when offline, rate limited or the release is a pre-release", async () => {
    expect((await checkWithFetch("1.0.0", vi.fn(async () => { throw new Error("offline"); }) as unknown as typeof fetch)).state).toBe("failed");
    expect((await checkWithFetch("1.0.0", answer({}, false))).state).toBe("failed");
    expect((await checkWithFetch("1.0.0", answer({ tag_name: "v2.0.0", prerelease: true }))).state).toBe("failed");
  });
  it("sends no credentials", async () => {
    const f = answer({ tag_name: "v1.0.0" });
    await checkWithFetch("1.0.0", f);
    expect((f as unknown as ReturnType<typeof vi.fn>).mock.calls[0]![1]).toMatchObject({ credentials: "omit" });
  });
});

const updateButton = () => screen.queryByRole("button", { name: "Update Available" });
const updateFlyout = () => screen.queryByRole("dialog", { name: "Update Available" });

describe("update button and UPDATE AVAILABLE flyout", () => {
  it("a newer version shows the title-bar button; the flyout downloads; Close dismisses and is remembered", async () => {
    const user = userEvent.setup();
    const api = fast();
    api.appUpdate.status = { ...NEWER };
    const first = renderApp("/characters", { api });
    await user.click(await screen.findByRole("button", { name: "Update Available" }));
    const flyout = await screen.findByRole("dialog", { name: "Update Available" });
    expect(flyout.textContent).toContain("Borealis 1.0.1");
    expect(flyout.textContent).toContain("You have version 1.0.0");
    await user.click(within(flyout).getByRole("button", { name: "Download" }));
    await waitFor(() => expect(api.appUpdate.opened).toBe(1));
    await user.click(within(flyout).getByRole("button", { name: "Close" }));
    expect(updateFlyout()).toBeNull();
    expect(updateButton()).toBeNull();
    first.unmount();
    // A new launch: the same version stays hidden, a newer one comes back.
    renderApp("/characters", { api });
    await screen.findByRole("contentinfo", { name: "Status" });
    await waitFor(() => expect(updateButton()).toBeNull());
    await act(async () => { await api.checkAppUpdate(); });
    expect(updateButton()).toBeNull();
    api.appUpdate.status = { ...NEWER, latest: "1.0.2" };
    await act(async () => { await api.checkAppUpdate(); });
    await user.click(await screen.findByRole("button", { name: "Update Available" }));
    expect((await screen.findByRole("dialog", { name: "Update Available" })).textContent).toContain("1.0.2");
  });

  it("shows the release notes as text in the description pane, never as HTML", async () => {
    const user = userEvent.setup();
    const api = fast();
    api.appUpdate.status = { ...NEWER, notes: "## What's new\n- Faster **imports**\n- <img src=x onerror=alert(1)>" };
    renderApp("/characters", { api });
    await user.click(await screen.findByRole("button", { name: "Update Available" }));
    const flyout = await screen.findByRole("dialog", { name: "Update Available" });
    const notes = within(flyout).getByTestId("update-notes");
    expect(notes.textContent).toContain("What's new");
    expect(notes.querySelector("strong")!.textContent).toBe("imports");
    expect(notes.querySelectorAll("li")).toHaveLength(2);
    expect(notes.querySelector("img")).toBeNull();
    expect(notes.textContent).toContain("<img src=x onerror=alert(1)>");
  });

  it("a markdown table in the release notes is a table, not a run of pipes (B-08)", async () => {
    const user = userEvent.setup();
    const api = fast();
    api.appUpdate.status = { ...NEWER, notes: "Pick a file:\n\n| Platform | File |\n|---|:---:|\n| macOS | `a.dmg` |\n| Android | <b>b.apk</b> |\n\nDone." };
    renderApp("/characters", { api });
    await user.click(await screen.findByRole("button", { name: "Update Available" }));
    const notes = within(await screen.findByRole("dialog", { name: "Update Available" })).getByTestId("update-notes");
    const table = notes.querySelector("table")!;
    expect([...table.querySelectorAll("th")].map((c) => c.textContent)).toEqual(["Platform", "File"]);
    expect([...table.querySelectorAll("tbody tr")].map((r) => [...r.querySelectorAll("td")].map((c) => c.textContent))).toEqual([["macOS", "a.dmg"], ["Android", "<b>b.apk</b>"]]);
    expect(table.querySelector("code")!.textContent).toBe("a.dmg");
    expect(table.querySelector("b")).toBeNull();
    expect(notes.textContent).not.toContain("|---");
  });

  it("a missing body shows no empty notes pane", async () => {
    const user = userEvent.setup();
    const api = fast();
    api.appUpdate.status = { ...NEWER };
    renderApp("/characters", { api });
    await user.click(await screen.findByRole("button", { name: "Update Available" }));
    const flyout = await screen.findByRole("dialog", { name: "Update Available" });
    expect(within(flyout).queryByTestId("update-notes")).toBeNull();
    expect(flyout.textContent).toContain("You have version 1.0.0");
  });

  it("sits in the title-bar commands left of the website button and pushes nothing down", async () => {
    const api = fast();
    api.appUpdate.status = { ...NEWER };
    const { container } = renderApp("/characters", { api });
    const button = await screen.findByRole("button", { name: "Update Available" });
    const cmds = container.querySelector(".aur-wincmds")!;
    const labels = [...cmds.querySelectorAll("button")].map((b) => b.getAttribute("aria-label"));
    expect(labels.indexOf("Update Available")).toBe(labels.indexOf("Project website") - 1);
    expect(cmds.contains(button)).toBe(true);
    expect(screen.queryByRole("region", { name: "Update Available" })).toBeNull();
    expect(container.querySelector(".aur-update")).toBeNull();
  });

  it("Escape closes the flyout and keeps the button", async () => {
    const user = userEvent.setup();
    const api = fast();
    api.appUpdate.status = { ...NEWER };
    renderApp("/characters", { api });
    await user.click(await screen.findByRole("button", { name: "Update Available" }));
    await screen.findByRole("dialog", { name: "Update Available" });
    await user.keyboard("{Escape}");
    await waitFor(() => expect(updateFlyout()).toBeNull());
    expect(updateButton()).not.toBeNull();
  });

  it("phones show the button in the title bar next to Settings", async () => {
    const phone = (q: string) => ({
      matches: true, media: q, onchange: null, addEventListener: () => undefined, removeEventListener: () => undefined,
      addListener: () => undefined, removeListener: () => undefined, dispatchEvent: () => false,
    }) as MediaQueryList;
    Object.defineProperty(window, "matchMedia", { configurable: true, writable: true, value: phone });
    try {
      const user = userEvent.setup();
      const api = fast();
      api.appUpdate.status = { ...NEWER };
      const { container } = renderApp("/characters", { api });
      const button = await screen.findByRole("button", { name: "Update Available" });
      expect(container.querySelector(".aur-title.mobile")!.contains(button)).toBe(true);
      await user.click(button);
      expect((await screen.findByRole("dialog", { name: "Update Available" })).textContent).toContain("Borealis 1.0.1");
    } finally { Reflect.deleteProperty(window, "matchMedia"); }
  });

  it("does not show when you are up to date", async () => {
    renderApp("/characters", { api: fast() });
    await screen.findByRole("contentinfo", { name: "Status" });
    expect(updateButton()).toBeNull();
  });
});

describe("opening the download page", () => {
  it("leaves the web view alone when the backend opens the browser (desktop and phones)", async () => {
    const api = fast();
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    try {
      await openDownload(api);
      expect(api.appUpdate.opened).toBe(1);
      expect(open).not.toHaveBeenCalled();
    } finally { open.mockRestore(); }
  });

  it("falls back to window.open only when the backend cannot open it", async () => {
    const api = fast();
    vi.spyOn(api, "openDownloadPage").mockRejectedValue(new Error("no opener"));
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    try {
      await openDownload(api);
      expect(open).toHaveBeenCalledWith("https://borealisbuilder.com/#download", "_blank", "noopener,noreferrer");
    } finally { open.mockRestore(); }
  });
});

describe("Borealis Settings: updates", () => {
  it("explains the setting, saves it, and Check now reports the latest version", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters");
    await openBorealisSettings(user, "General");
    const box = screen.getByRole("checkbox", { name: "Check for new versions of Borealis" });
    expect((box as HTMLInputElement).checked).toBe(true);
    expect(screen.getByText(/Only asks GitHub for the latest version number/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Check now" }));
    expect(await screen.findByText("You have the latest version (1.0.1).")).toBeTruthy();
    expect(api.appUpdate.requests).toBe(1);
    await user.click(box);
    await waitFor(async () => expect((await api.getSettings()).checkAppUpdates).toBe(false));
  });

  it("Check now names the new version", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/characters", { api });
    await openBorealisSettings(user, "General");
    api.appUpdate.status = { ...NEWER };
    await user.click(screen.getByRole("button", { name: "Check now" }));
    expect(await screen.findByText("Borealis 1.0.1 is available (you have 1.0.0).")).toBeTruthy();
  });
});
