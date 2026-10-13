import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { framePreviewHtml, PREVIEW_CSS } from "../pages/sheet/previewFrame";
import { renderApp, seededId } from "./utils";

const open = async (route: string) => {
  const api = new MockApi({ rng: () => 0.5 });
  const id = await seededId(api, "Brannoc");
  renderApp(route, { api, openCharacterId: id });
  return { api, id };
};

describe("Manage", () => {
  it("shows the sheet tab set and none of Treasure, Options or Rules", async () => {
    await open("/manage");
    const names = (await screen.findAllByRole("tab")).map((t) => t.textContent);
    const manage = names.slice(names.indexOf("Character"));
    expect(manage).toEqual(["Character", "Backstory", "Notes", "Allies & Organizations", "Attacks & Spellcasting"]);
  });

  it("has no inline portrait section and uses combos for gender, alignment, deity, eyes, skin and hair", async () => {
    await open("/manage");
    await screen.findByLabelText("Character name");
    expect(screen.queryByRole("heading", { name: "Portrait" })).toBeNull();
    for (const label of ["Gender", "Alignment", "Deity (optional)", "Eyes", "Skin", "Hair"]) {
      expect(screen.getByLabelText(label).tagName).toBe("SELECT");
    }
  });

  it("steps current experience with the flat + and - buttons", async () => {
    const user = userEvent.setup();
    const { api, id } = await open("/manage");
    const before = (await api.getSnapshot({ characterId: id })).level.xp;
    await user.click(await screen.findByRole("button", { name: "Increase experience" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).level.xp).toBe(before + 100));
    await user.click(screen.getByRole("button", { name: "Decrease experience" }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).level.xp).toBe(before));
  });

  it("marks the alignment label with a check once an alignment is chosen", async () => {
    const user = userEvent.setup();
    await open("/manage");
    const select = await screen.findByLabelText("Alignment");
    const label = select.closest(".mg-f")!;
    expect(within(label as HTMLElement).queryByRole("img", { name: "set" })).toBeNull();
    await user.selectOptions(select, "Lawful Good");
    expect(await within(label as HTMLElement).findByRole("img", { name: "set" })).toBeTruthy();
  });

  it("offers the attack toolbar and no empty-state text", async () => {
    const user = userEvent.setup();
    await open("/manage");
    await user.click(await screen.findByRole("tab", { name: "Attacks & Spellcasting" }));
    const bar = await screen.findByRole("toolbar", { name: "Attack actions" });
    expect(within(bar).getByRole("button", { name: "Add Attack" })).toBeTruthy();
    expect(within(bar).getByRole("button", { name: "Move selected attack up" })).toBeTruthy();
    expect(screen.queryByText("No attacks yet.")).toBeNull();
  });
});

describe("Character sheet", () => {
  it("renders the generated sheet in a sandboxed frame on a white page, with no format toggle", async () => {
    const user = userEvent.setup();
    await open("/sheet");
    const link = await screen.findByRole("link", { name: /character sheet settings/i });
    expect(link.getAttribute("href")).toContain("/settings");
    expect(screen.queryByRole("radio", { name: "HTML" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Sheet options" })).toBeNull();
    await user.click(await screen.findByRole("button", { name: "Generate Preview" }));
    const frame = (await screen.findByTitle("Character sheet preview")) as HTMLIFrameElement;
    expect(frame.getAttribute("sandbox")).toBe("");
    expect(frame.srcdoc).toContain('<section class="page"');
    expect(frame.srcdoc).toContain(PREVIEW_CSS);
  });

  it("keeps the armor class field at 42 px high in the additional fields", async () => {
    await open("/sheet");
    const box = await screen.findByLabelText(/Armor class \(conditional field\)/);
    expect(box.tagName).toBe("TEXTAREA");
    expect(box.closest(".v4-sheet-sec")).toBeTruthy();
  });

  it("framePreviewHtml injects the page styling before </head> and tolerates a headless document", () => {
    const html = framePreviewHtml("<html><head><title>t</title></head><body class=\"sheet\"></body></html>");
    expect(html.indexOf("<style>")).toBeLessThan(html.indexOf("</head>"));
    expect(html).toContain("background:#131313");
    expect(framePreviewHtml("<p>x</p>").startsWith("<style>")).toBe(true);
  });
});
