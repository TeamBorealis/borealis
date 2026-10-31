import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MENUS } from "../components/shell/sections";
import { EXTERNAL_LINKS, LINKS, openLink } from "../lib/links";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

// GitHub #9 (decision U15): the Help menu and the title-bar globe open real pages in the system browser, through one
// backend command that opens only addresses on a fixed list. No entry says "not published yet" any more.
async function openHelp(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("menuitem", { name: "Help" }));
}

describe("U15 Help menu links", () => {
  it("News, Releases and Support Page open their pages through open_link", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters");
    const open = vi.spyOn(api, "openLink");
    for (const [entry, url] of [
      ["News", "https://borealisbuilder.com/changelog"],
      ["Releases", "https://github.com/TeamBorealis/borealis/releases"],
      ["Support Page", "https://discord.gg/r4K4D3AJpZ"],
    ] as const) {
      await openHelp(user);
      await user.click(await screen.findByRole("menuitem", { name: entry }));
      await waitFor(() => expect(open).toHaveBeenLastCalledWith({ url }));
    }
    expect(open).toHaveBeenCalledTimes(3);
    expect(api.openedLinks).toEqual([
      "https://borealisbuilder.com/changelog",
      "https://github.com/TeamBorealis/borealis/releases",
      "https://discord.gg/r4K4D3AJpZ",
    ]);
    expect(screen.queryByText(/not published yet/i)).toBeNull();
  });

  it("Support Borealis is hidden until a donation page exists", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await openHelp(user);
    await screen.findByRole("menuitem", { name: "News" });
    expect(screen.queryByRole("menuitem", { name: "Support Borealis" })).toBeNull();
  });

  it("Get Started and Additional Content stay in-app screens", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/content/sources");
    const open = vi.spyOn(api, "openLink");
    await openHelp(user);
    await user.click(await screen.findByRole("menuitem", { name: "Additional Content" }));
    await openHelp(user);
    await user.click(await screen.findByRole("menuitem", { name: "Get Started" }));
    expect(await screen.findByRole("group", { name: "Seraphine Vale" })).toBeTruthy();
    expect(open).not.toHaveBeenCalled();
  });

  it("the title-bar globe opens the project website", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/characters");
    const open = vi.spyOn(api, "openLink");
    await user.click(await screen.findByRole("button", { name: "Project website" }));
    await waitFor(() => expect(open).toHaveBeenCalledWith({ url: "https://borealisbuilder.com/" }));
    expect(screen.queryByText(/not published yet/i)).toBeNull();
  });

  it("every menu link is on the fixed list, and the mock refuses anything else like the backend", async () => {
    const links = MENUS.flatMap((m) => m.entries).flatMap((e) => (e.link ? [e.link] : []));
    expect(links.length).toBe(3);
    for (const l of [...links, LINKS.website]) expect(EXTERNAL_LINKS).toContain(l);
    const api = new MockApi({ aiChunkDelayMs: 2 });
    await expect(api.openLink({ url: "https://example.com/" })).rejects.toMatchObject({ code: "invalid_argument" });
    await expect(api.openLink({ url: `${LINKS.news}/../evil` })).rejects.toMatchObject({ code: "invalid_argument" });
    expect(api.openedLinks).toEqual([]);
  });

  it("falls back to window.open only when the backend cannot open the link", async () => {
    const api = new MockApi({ aiChunkDelayMs: 2 });
    vi.spyOn(api, "openLink").mockRejectedValue(new Error("no opener"));
    const win = vi.spyOn(window, "open").mockReturnValue(null);
    try {
      await openLink(api, LINKS.releases);
      expect(win).toHaveBeenCalledWith(LINKS.releases, "_blank", "noopener,noreferrer");
    } finally { win.mockRestore(); }
  });

  it("does not fall back to window.open when the backend refuses the address (review L4)", async () => {
    const api = new MockApi({ aiChunkDelayMs: 2 });
    vi.spyOn(api, "openLink").mockRejectedValue({ code: "invalid_argument", message: "Borealis only opens its own web pages." });
    const win = vi.spyOn(window, "open").mockReturnValue(null);
    try {
      await openLink(api, "https://example.com/");
      expect(win).not.toHaveBeenCalled();
    } finally { win.mockRestore(); }
  });
});
