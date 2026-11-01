import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { setPhoneOsForTests } from "../lib/phone";
import { renderApp } from "./utils";

// iPad (and Android tablets) get the wide shell. No mobile window can be minimised, maximised or closed, so the title
// bar has no window buttons there and File has no Exit Application (window.close() did nothing). Desktop keeps both.
afterEach(() => { setPhoneOsForTests(null); });

async function fileMenuLabels(user: ReturnType<typeof userEvent.setup>): Promise<string[]> {
  await user.click(await screen.findByRole("menuitem", { name: "File" }));
  return within(await screen.findByRole("menu")).getAllByRole("menuitem").map((m) => m.textContent ?? "");
}

describe("wide shell on a tablet", () => {
  it.each(["ios", "android"] as const)("%s: no window buttons and no Exit Application", async (os) => {
    setPhoneOsForTests(os);
    const user = userEvent.setup();
    renderApp("/characters");
    await screen.findByRole("menuitem", { name: "File" });
    for (const name of ["Minimize", "Maximize", "Close window"]) expect(screen.queryByRole("button", { name })).toBeNull();
    expect((await fileMenuLabels(user)).some((l) => l.includes("Exit Application"))).toBe(false);
  });

  it("desktop keeps the window buttons and Exit Application", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await screen.findByRole("menuitem", { name: "File" });
    for (const name of ["Minimize", "Maximize", "Close window"]) expect(screen.getByRole("button", { name })).toBeTruthy();
    expect((await fileMenuLabels(user)).some((l) => l.includes("Exit Application"))).toBe(true);
  });
});
