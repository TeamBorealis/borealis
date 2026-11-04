import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { renderApp, seededId } from "./utils";

// B-159 / GitHub #20 (PH2, suggested by Tjop Vaoxad on Discord): on a phone the item list and the inventory are page-scrolled,
// so Add/Buy and the inventory toolbar sat below long lists. On phones each bar is pinned (sticky) above the bottom tabs while
// its own list is on screen. jsdom has no layout, so these tests check the structure and the CSS; the pixels were checked in
// a browser at 390x844 and 820x1180 (specs/handoffs/b159-phone-item-bars.md).

const css = (file: string) => readFileSync(resolve(__dirname, "..", file), "utf8");

function asPhone() {
  document.documentElement.classList.add("phone");
  setPhoneForTests(true);
}

afterEach(() => {
  setPhoneForTests(false);
  document.documentElement.classList.remove("phone");
});

async function openEquipment() {
  const api = new MockApi({ rng: () => 0.5 });
  const id = await seededId(api, "Brannoc");
  renderApp("/equipment", { api, openCharacterId: id });
  const add = await screen.findByRole("button", { name: "Add" });
  const layout = add.closest(".eq-layout") as HTMLElement;
  const toolbar = await screen.findByRole("toolbar", { name: "Inventory actions" });
  return { api, id, add, layout, toolbar };
}

/** The direct children of the equipment layout, by their first class (the category nav is hidden on phones). */
const layoutParts = (layout: HTMLElement) => [...layout.children].map((c) => c.classList[0]);

describe("Phone equipment action bars stay pinned (B-159, PH2)", () => {
  it("on a phone, Add/Buy stand outside the tools row as a pinned bar, between the item list and the tools row", async () => {
    // Arrange
    asPhone();

    // Act
    const { add, layout } = await openEquipment();

    // Assert
    const cmds = add.closest(".eq-cmds") as HTMLElement;
    expect(cmds.className).toBe("eq-cmds eq-pin");
    expect(cmds.parentElement).toBe(layout);
    expect(layoutParts(layout)).toEqual(["eq-cats", "eq-items", "eq-cmds", "eq-tools", "eq-inv"]);
    expect(within(cmds).getByRole("button", { name: "Buy" })).toBeTruthy();
    expect(within(cmds).getByRole("textbox", { name: "Add quantity" })).toBeTruthy();
    expect(within(cmds).getByRole("textbox", { name: "Buy quantity" })).toBeTruthy();
    // Filter, source and Manage coins stay in the tools row, unpinned.
    const tools = layout.querySelector(":scope > .eq-tools") as HTMLElement;
    expect(tools.querySelector(".eq-cmds")).toBeNull();
    expect(tools.querySelector(".eq-pin")).toBeNull();
    expect(within(tools).getByRole("textbox", { name: "Filter text" })).toBeTruthy();
    expect(within(tools).getByRole("button", { name: /^Manage coins/ })).toBeTruthy();
  });

  it("on a phone, the inventory toolbar is the inventory's pinned bar, and only the two bars are pinned", async () => {
    // Arrange
    asPhone();

    // Act
    const { layout, toolbar } = await openEquipment();

    // Assert
    expect(toolbar.className).toBe("eq-inv-actions eq-pin");
    expect(toolbar.parentElement?.classList.contains("eq-inv")).toBe(true);
    expect([...layout.querySelectorAll(".eq-pin")].map((e) => e.classList[0])).toEqual(["eq-cmds", "eq-inv-actions"]);
    expect(within(toolbar).getAllByRole("button").map((b) => b.textContent)).toEqual(["Details", "Up", "Down", "Delete", "Delete all"]);
  });

  it("on a phone, the pinned Add still adds the picked item", async () => {
    // Arrange
    asPhone();
    const user = userEvent.setup();
    const { api, id, add } = await openEquipment();
    const before = (await api.getSnapshot({ characterId: id })).inventory.items.length;
    await user.click(screen.getByRole("button", { name: /Rope/ }));

    // Act
    await waitFor(() => expect((add as HTMLButtonElement).disabled).toBe(false));
    await user.click(add);

    // Assert
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).inventory.items.length).toBe(before + 1));
  });

  it("on desktop, the DOM is as before: Add/Buy lead the tools row and nothing is pinned", async () => {
    // Act
    const { add, layout, toolbar } = await openEquipment();

    // Assert
    const cmds = add.closest(".eq-cmds") as HTMLElement;
    expect(cmds.className).toBe("eq-cmds");
    expect(cmds.parentElement?.className).toBe("eq-tools");
    expect(cmds.parentElement?.firstElementChild).toBe(cmds);
    expect(layoutParts(layout)).toEqual(["eq-cats", "eq-items", "eq-tools", "eq-inv"]);
    expect(toolbar.className).toBe("eq-inv-actions");
    expect(layout.querySelector(".eq-pin")).toBeNull();
  });

  it("pins with CSS sticky at the bottom of the page scroller, on phones only, and keeps room for scrolled-to rows", () => {
    const gear = css("styles/phone/gear.css");
    expect(gear).toMatch(/html\.phone \.eq-pin \{ position: sticky; bottom: var\(--eq-pin-gap\);/);
    expect(gear).toMatch(/html\.phone \.eq-layout \.eq-grid tbody tr \{ scroll-margin-bottom: var\(--eq-pin-room\); \}/);
    // Lists stay page-scrolled on phones (no nested scroller, TV2 C11/C16): the sticky bars rely on it.
    expect(gear).toMatch(/html\.phone \.eq-scroll, html\.phone \.eq-inv-scroll \{[^}]*overflow: visible; \}/);
    // Every pinning rule is phone-scoped; the desktop stylesheet knows nothing of it (U7).
    for (const line of gear.split("\n").filter((l) => l.includes("eq-pin") && l.includes("{"))) expect(line.startsWith("html.phone ")).toBe(true);
    expect(css("pages/equipment/equipment.css")).not.toMatch(/eq-pin|sticky/);
  });
});
