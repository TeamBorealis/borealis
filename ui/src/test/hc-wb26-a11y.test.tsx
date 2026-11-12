import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { HomebrewRef } from "../api/contract";
import type { MockApi } from "../api/mock/MockApi";
import { ExplainStat } from "../components/ExplainStat";
import { setPhoneForTests } from "../lib/phone";
import { RefField } from "../pages/homebrew/pickers/RefField";
import { StatisticField } from "../pages/homebrew/pickers/StatisticPicker";
import { mockApi, renderControl, Stateful } from "./hc-wb10-harness";
import { renderApp } from "./utils";

// Homebrew WB-26, the accessibility pass: buttons the phones use never carry aria-haspopup (iOS WebKit then drops the
// button trait, so VoiceOver and XCTest see "other"; main fixed the phone select the same way, d05b7020); the New element
// kinds are one radio group with arrow keys; Enter in a flyout's name field makes it; a check field is Aurora's
// DescriptionCheckBox (its name is the box's label, beside it); the element table names every column; and focus lands in
// the new element's form, or on the pack's heading when a pack opens.

afterEach(() => act(() => setPhoneForTests(false)));

const editor = () => screen.getByRole("region", { name: "Element editor" });

async function workspace(api: MockApi = mockApi()) {
  const pack = await api.homebrewCreatePack({ request: { name: "Ember Isles", code: "EMB", edition: "dnd2024" } });
  renderApp(`/homebrew/${pack.id}`, { api });
  await screen.findAllByRole("button", { name: /^New( element)?$/ });
  return api;
}

async function newElement(user: ReturnType<typeof userEvent.setup>, kind: RegExp, name: string) {
  await user.click((await screen.findAllByRole("button", { name: /^New( element)?$/ }))[0]!);
  const flyout = await screen.findByRole("dialog", { name: "New element" });
  await user.click(within(flyout).getByRole("radio", { name: kind }));
  await user.type(within(flyout).getByLabelText("Name"), name);
  await user.click(within(flyout).getByRole("button", { name: "Create" }));
  await waitFor(() => expect((document.getElementById("hb-f-name-input") as HTMLInputElement | null)?.value).toBe(name));
}

describe("WB-26 aria-haspopup (iOS VoiceOver: a button with it is not a button)", () => {
  it("no source file outside the desktop menu bar puts aria-haspopup on a control", () => {
    // Arrange: every component source (tests excluded)
    const sources = import.meta.glob(["../**/*.tsx", "!../**/*.test.tsx", "!../test/**"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;
    // Act
    const carrying = Object.entries(sources).filter(([, text]) => /aria-haspopup\s*=/.test(text)).map(([file]) => file.replace(/^\.\.\//, ""));
    // Assert: the MenuBar's top items are role=menuitem on desktop only. SelectSheet's came off on main (d05b7020) and
    // is checked here too since the merge (its comment names the attribute, so the match is on the attribute itself).
    expect(carrying).toEqual(["components/shell/MenuBar.tsx"]);
  });

  it("the Explain trigger, a picker's Pick button and the statistic picker's button are plain buttons", async () => {
    // Arrange
    act(() => setPhoneForTests(true));
    await renderControl(
      <>
        <ExplainStat stat="proficiency" label="Proficiency" value="+2">+2</ExplainStat>
        <Stateful initial={[] as HomebrewRef[]}>{(v, set) => <RefField label="Languages" target={{ types: ["Language"], multiple: true }} value={v} onChange={set} />}</Stateful>
        <Stateful initial={""}>{(v, set) => <StatisticField id="st" label="Statistic" value={v} onChange={set} />}</Stateful>
      </>,
    );
    // Act
    const buttons = [
      screen.getByRole("button", { name: "Explain Proficiency, +2" }),
      screen.getByRole("button", { name: "Pick: Languages" }),
      screen.getByRole("button", { name: "Pick: Statistic" }),
    ];
    // Assert
    for (const b of buttons) expect(b.hasAttribute("aria-haspopup")).toBe(false);
  });

  it("a class's LEVELS rows on phones are plain buttons that open the level's sheet", async () => {
    // Arrange
    act(() => setPhoneForTests(true));
    const user = userEvent.setup();
    await workspace();
    await newElement(user, /^Class A class/, "Ember Knight");
    // Act
    await user.click(within(editor()).getByRole("tab", { name: "LEVELS" }));
    const row = within(editor()).getByRole("button", { name: /^Level 4/ });
    // Assert: a plain button, its name and detail read as two phrases
    expect(row.hasAttribute("aria-haspopup")).toBe(false);
    expect(row.getAttribute("aria-label")).toBe("Level 4, +2 · Ability Score Improvement");
    await user.click(row);
    expect(await screen.findByRole("dialog", { name: "Level 4" })).toBeTruthy();
  });
});

describe("WB-26 keyboard", () => {
  it("New element: the kinds are one Tab stop, the arrow keys choose the next kind and Home / End the first and last", async () => {
    // Arrange
    const user = userEvent.setup();
    await workspace();
    await user.click((await screen.findAllByRole("button", { name: /^New( element)?$/ }))[0]!);
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    const kinds = within(within(flyout).getByRole("radiogroup", { name: "Kind" })).getAllByRole("radio");
    // Act: focus the chosen kind, then ArrowDown, then End
    kinds[0]!.focus();
    await user.keyboard("{ArrowDown}");
    const afterDown = document.activeElement;
    await user.keyboard("{End}");
    // Assert
    expect(kinds.filter((k) => k.tabIndex === 0)).toHaveLength(1);
    expect(afterDown).toBe(kinds[1]);
    expect(document.activeElement).toBe(kinds[kinds.length - 1]);
    expect(kinds[kinds.length - 1]!.getAttribute("aria-checked")).toBe("true");
    expect(kinds[kinds.length - 1]!.tabIndex).toBe(0);
  });

  it("Enter in New pack's name field makes the pack, and the workspace starts at the pack's heading", async () => {
    // Arrange
    const user = userEvent.setup();
    const api = mockApi();
    renderApp("/homebrew", { api });
    await user.click((await screen.findAllByRole("button", { name: "New pack" }))[0]!);
    const flyout = await screen.findByRole("dialog", { name: "New pack" });
    // Act
    await user.type(within(flyout).getByLabelText("Name"), "Ember Isles{Enter}");
    // Assert
    const heading = await screen.findByRole("heading", { name: "Homebrew: Ember Isles" });
    await waitFor(() => expect(document.activeElement).toBe(heading));
    expect((await api.listPacks()).some((p) => p.name === "Ember Isles")).toBe(true);
  });

  it("Enter in New element's name field makes the element, and focus lands in its Name field", async () => {
    // Arrange
    const user = userEvent.setup();
    await workspace();
    await user.click((await screen.findAllByRole("button", { name: /^New( element)?$/ }))[0]!);
    const flyout = await screen.findByRole("dialog", { name: "New element" });
    // Act
    await user.type(within(flyout).getByLabelText("Name"), "Ember Lance{Enter}");
    // Assert
    await waitFor(() => expect(document.activeElement?.id).toBe("hb-f-name-input"));
    expect((document.activeElement as HTMLInputElement).value).toBe("Ember Lance");
  });
});

describe("WB-26 form and list semantics", () => {
  it("a check field is the DescriptionCheckBox: the name beside the box is its label, the help its description", async () => {
    // Arrange
    const user = userEvent.setup();
    await workspace();
    await newElement(user, /^Spell A spell/, "Ember Lance");
    // Act
    const ritual = within(editor()).getByRole("checkbox", { name: "Ritual" }) as HTMLInputElement;
    await user.click(within(editor()).getByText("Ritual"));
    // Assert: the label sits beside the box (same row), a click on it toggles the box
    expect(ritual.closest(".hb-dcheck")?.querySelector(".hb-dcheck-head")?.textContent).toBe("Ritual");
    expect(ritual.checked).toBe(true);
  });

  it("the element table names every column and does not claim aria-multiselectable", async () => {
    // Arrange
    const user = userEvent.setup();
    await workspace();
    await newElement(user, /^Spell A spell/, "Ember Lance");
    // Act
    await user.click(screen.getByRole("button", { name: "Select" }));
    const table = screen.getByRole("table", { name: "Elements" });
    // Assert
    expect(table.hasAttribute("aria-multiselectable")).toBe(false);
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Chosen", "Name", "Edition", "Marks"]);
  });

  it("desktop: the Expert switch is in the command row, the sub-tab band holds only the tabs", async () => {
    // Arrange
    const user = userEvent.setup();
    await workspace();
    // Act
    await newElement(user, /^Class A class/, "Ember Knight");
    // Assert
    const band = editor().querySelector<HTMLElement>(".hb-band")!;
    expect(within(band).getAllByRole("tab").map((t) => t.textContent)).toEqual(["GENERAL", "PROFICIENCIES", "LEVELS", "SPELLCASTING", "MULTICLASS", "EQUIPMENT", "RULES", "DESCRIPTION", "DATA"]);
    expect(within(band).queryByRole("switch")).toBeNull();
    expect(within(editor().querySelector<HTMLElement>(".hb-cmdrow")!).getByRole("switch", { name: "Expert fields" })).toBeTruthy();
  });
});
