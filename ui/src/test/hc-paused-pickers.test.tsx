import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { HomebrewRef } from "../api/contract";
import { setPhoneForTests } from "../lib/phone";
import { RefField } from "../pages/homebrew/pickers/RefField";
import { parseRules, readCss } from "./cssRules";
import { mockApi, renderControl, Stateful } from "./hc-wb10-harness";

// The paused 1.5.0 creator picker item ("phone multi-select looks single-choice; desktop repeats chosen rows; rows show
// 'That element does not exist'"), checked again on main: a multi-select picker shows a check box per row on desktop as on
// phones (spec 5.1: "Multi-select pickers show check boxes (Magic grid)"), the picks are not listed a second time under the
// list, a checked phone mark is drawn checked, and the web mock describes the core rows item fields point at.

afterEach(() => act(() => setPhoneForTests(false)));

async function multiField(types: string[], onValue = vi.fn()) {
  await renderControl(
    <Stateful initial={[] as HomebrewRef[]} onValue={onValue}>
      {(v, set) => <RefField label="Properties" target={{ types, multiple: true }} value={v} onChange={set} />}
    </Stateful>,
  );
  return onValue;
}

describe("multi-select reference picker on desktop", () => {
  it("gives each row a check box that checks and clears it, and does not list the picks again under the list", async () => {
    // Arrange
    const user = userEvent.setup();
    const onValue = await multiField(["Language"]);
    await user.click(screen.getByRole("button", { name: "Pick: Properties" }));
    const dialog = await screen.findByRole("dialog");
    const list = await within(dialog).findByRole("list", { name: "Properties options" });
    const check = (name: string) => within(list).getByRole("checkbox", { name: `Choose ${name}` }) as HTMLInputElement;
    await within(list).findByRole("checkbox", { name: "Choose Dwarvish" });
    // Act
    await user.click(check("Dwarvish"));
    await user.click(check("Elvish"));
    await user.click(check("Dwarvish"));
    // Assert
    expect(check("Dwarvish").checked).toBe(false);
    expect(check("Elvish").checked).toBe(true);
    expect(dialog.querySelector(".pick-footer")).toBeNull();
    expect(within(dialog).queryByRole("button", { name: "Clear Elvish" })).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "OK" }));
    expect(onValue.mock.lastCall![0].map((r: HomebrewRef) => r.name)).toEqual(["Elvish"]);
  });

  it("a checked row is marked by its check box, not by the highlight the described row has", async () => {
    const user = userEvent.setup();
    await multiField(["Language"]);
    await user.click(screen.getByRole("button", { name: "Pick: Properties" }));
    const list = await screen.findByRole("list", { name: "Properties options" });
    await user.click(await within(list).findByRole("checkbox", { name: "Choose Giant" }));
    const giant = within(list).getByRole("button", { name: /^Giant/ }).closest("li")!;
    expect(giant.classList.contains("selected")).toBe(false);
    expect(within(list).getByRole("button", { name: /^Giant/ }).getAttribute("aria-pressed")).toBe("true");
  });

  it("a single-choice picker keeps the Build picker's rows: no check boxes", async () => {
    const user = userEvent.setup();
    await renderControl(<RefField label="Properties" target={{ types: ["Language"] }} value={[]} onChange={() => undefined} />);
    await user.click(screen.getByRole("button", { name: "Pick: Properties" }));
    const list = await screen.findByRole("list", { name: "Properties options" });
    await within(list).findByRole("button", { name: /^Dwarvish/ });
    expect(within(list).queryAllByRole("checkbox")).toHaveLength(0);
  });
});

describe("the reference picker's row colours", () => {
  it("the dialog has the Build picker's colours, so the described row is highlighted (--bp-sel was only set on .bp)", () => {
    const rules = parseRules(readCss("pages/builder/builder.css"));
    const dark = rules.filter((r) => r.selectors.includes(".hb-picker") && r.atRules.length === 0).map((r) => r.body).join(";");
    for (const v of ["--bp-sel", "--bp-box", "--bp-rule"]) expect(dark, v).toMatch(new RegExp(`${v}:`));
    expect(rules.some((r) => r.selectors.includes(':root[data-theme="light"] .hb-picker') && /--bp-sel:/.test(r.body))).toBe(true);
  });
});

describe("multi-select reference picker on phones", () => {
  it("draws a checked mark checked: the phone rule keys on the attribute the mark carries", async () => {
    // Arrange
    act(() => setPhoneForTests(true));
    const user = userEvent.setup();
    await multiField(["Language"]);
    await user.click(screen.getByRole("button", { name: "Pick: Properties" }));
    const list = await screen.findByRole("list", { name: "Properties options" });
    // Act
    await user.click(await within(list).findByRole("button", { name: /^Elvish/ }));
    // Assert: the mark says checked with aria-checked, and the phone CSS draws the check for exactly that
    const mark = within(list).getByRole("checkbox", { name: "Choose Elvish" });
    expect(mark.getAttribute("aria-checked")).toBe("true");
    expect(mark.hasAttribute("aria-pressed")).toBe(false);
    const rules = parseRules(readCss("styles/phone/lists.css"));
    const checked = rules.find((r) => r.selectors.includes('html.phone .pick-mark.is-check[aria-checked="true"]::after'));
    expect(checked?.body).toMatch(/mask/);
    expect(rules.some((r) => r.selectors.some((s) => s.includes(".is-check[aria-pressed")))).toBe(false);
  });

  it("does not list the picks again under the list either", async () => {
    act(() => setPhoneForTests(true));
    const user = userEvent.setup();
    await multiField(["Language"]);
    await user.click(screen.getByRole("button", { name: "Pick: Properties" }));
    const list = await screen.findByRole("list", { name: "Properties options" });
    await user.click(await within(list).findByRole("button", { name: /^Elvish/ }));
    expect(document.querySelector(".hb-picker .pick-footer")).toBeNull();
  });
});

describe("the web mock describes the core rows item fields point at", () => {
  it("getElement answers for a weapon property and a mastery row, as the app's core pack does", async () => {
    const api = mockApi();
    const finesse = await api.getElement({ elementId: "ID_INTERNAL_WEAPON_PROPERTY_FINESSE" });
    expect(finesse.name).toBe("Finesse");
    expect(finesse.sourceName).toBe("Core");
    const sap = await api.getElement({ elementId: "ID_2024_MASTERY_SAP" });
    expect(sap.name).toBe("Sap");
    expect(sap.rulesets).toEqual(["dnd2024"]);
  });

  it("the weapon property picker describes the clicked row instead of 'That element does not exist.'", async () => {
    const user = userEvent.setup();
    await multiField(["Weapon Property"]);
    await user.click(screen.getByRole("button", { name: "Pick: Properties" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(await within(dialog).findByRole("button", { name: /^Finesse/ }));
    await waitFor(() => expect(within(dialog).getByRole("heading", { name: "Finesse" })).toBeTruthy());
    expect(dialog.textContent).not.toContain("That element does not exist.");
  });
});
