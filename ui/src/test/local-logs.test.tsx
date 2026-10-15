import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

async function openDataFiles(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("menuitem", { name: "View" }));
  await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
  const dialog = await screen.findByRole("dialog", { name: "Application Settings" });
  await user.click(within(dialog).getByRole("tab", { name: "Data files" }));
  return dialog;
}

afterEach(() => { vi.restoreAllMocks(); Reflect.deleteProperty(navigator, "share"); });

describe("local logs in Settings (UP3)", () => {
  it("desktop: Open logs folder asks the backend to open the logs folder", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ rng: () => 0.5 });
    renderApp("/characters", { api });
    const dialog = await openDataFiles(user);
    expect(within(dialog).queryByRole("button", { name: "Share logs" })).toBeNull();
    await user.click(await within(dialog).findByRole("button", { name: "Open logs folder" }));
    await waitFor(() => expect(api.openedFolders).toEqual(["logs"]));
  });

  it("mobile: Share logs hands the recent log text to the share sheet", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ mobile: true, rng: () => 0.5 });
    api.recentLogs = "line one\nline two";
    const share = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "share", { value: share, configurable: true });
    renderApp("/settings?tab=data", { api });
    const dialog = await screen.findByRole("dialog", { name: "Application Settings" });
    expect(within(dialog).queryByRole("button", { name: "Open logs folder" })).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Share logs" }));
    await waitFor(() => expect(share).toHaveBeenCalledWith({ title: "Borealis logs", text: "line one\nline two" }));
  });
});
