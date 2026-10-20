import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { ImportStartRequest } from "../api/contract";
import { detectRulesContent, rulesContentRefusal } from "../pages/import/rulesContent";
import { stashImportFiles } from "../state/importInbox";
import { renderApp } from "./utils";

// User rule: Aurora and 5etools imports are deterministic and never involve AI. The AI importer
// refuses that content (and Borealis packs) and points to Additional Content. Synthetic fixtures only.
const AURORA = `<?xml version="1.0" encoding="utf-8"?>
<elements>
  <element name="Test Spark" type="Spell" source="Synthetic" id="ID_SYN_SPELL_TEST_SPARK">
    <description><p>A tiny spark.</p></description>
  </element>
</elements>`;
const AURORA_FRAGMENT = `<element name="Test Blade" type="Item" source="Synthetic" id="ID_SYN_ITEM_TEST_BLADE"></element>`;
const AURORA_INDEX = `<index><info><name>Syn</name></info><files><file name="syn.xml" url="https://example.invalid/syn.xml" /></files></index>`;
const FIVETOOLS = JSON.stringify({ spell: [{ name: "Test Spark", source: "SYN", page: 1, level: 0, entries: ["A tiny spark."] }] });
const FIVETOOLS_HOMEBREW = JSON.stringify({ _meta: { sources: [{ json: "SYN", abbreviation: "SYN" }] }, feat: [] });
const FIVETOOLS_FLUFF = JSON.stringify({ monsterFluff: [{ name: "Test Beast", source: "SYN", entries: ["Lore."] }] });
const FIVETOOLS_ENTITY = JSON.stringify({ name: "Test Feat", source: "SYN", page: 3, entries: ["You gain a thing."] });
const BOREALIS = JSON.stringify({ format: "borealis-content/1", elements: [] });
const BOREALIS_PACK = JSON.stringify({ format: "borealis-pack/1", id: "syn", name: "Syn", version: "1.0.0", rulesets: [], files: [] });

const fast = () => new MockApi({ aiChunkDelayMs: 2, importStepMs: 1, aiEnabled: true, rng: () => 0.5 });

function watched() {
  const api = fast();
  const seen: ImportStartRequest[] = [];
  const original = api.importStart.bind(api);
  api.importStart = (req) => { seen.push(req); return original(req); };
  return { api, seen };
}

describe("rules content detection", () => {
  it("recognises Aurora XML, 5etools data and Borealis packs", () => {
    for (const t of [AURORA, `\uFEFF ${AURORA}`, `Here it is:\n${AURORA}`, AURORA_FRAGMENT, AURORA_INDEX]) expect(detectRulesContent(t)).toBe("aurora");
    for (const t of [FIVETOOLS, FIVETOOLS_HOMEBREW, FIVETOOLS_FLUFF, FIVETOOLS_ENTITY, `[${FIVETOOLS_ENTITY}]`]) expect(detectRulesContent(t)).toBe("5etools");
    for (const t of [BOREALIS, BOREALIS_PACK]) expect(detectRulesContent(t)).toBe("borealis");
  });

  it("lets ordinary text and JSON through", () => {
    for (const t of [
      "Test Spark\nCantrip, evocation. A tiny spark of light.",
      "The <element> of surprise is key. An <index> of feats.",
      JSON.stringify({ name: "Test Spark", level: 0, text: "A tiny spark." }),
      JSON.stringify({ spell: "not a list" }),
      "[1, 2, 3]",
      "",
    ]) expect(detectRulesContent(t)).toBeNull();
  });

  it("matches the backend's wording", () => {
    expect(rulesContentRefusal("aurora", "The pasted text")).toBe("The pasted text is Aurora content. It is added as it is, without AI: save it as an .xml file and add it under Additional Content > Custom content.");
    expect(rulesContentRefusal("5etools", "\"a.json\"")).toBe("\"a.json\" is 5etools data. It is converted as it is, without AI: use Additional Content > 5etools import with your 5etools data folder.");
    expect(rulesContentRefusal("borealis", "\"p.json\"")).toBe("\"p.json\" is already a Borealis content pack. Add it as it is under Additional Content > Custom content.");
  });
});

describe("the AI importer refuses rules content", () => {
  it("a dropped 5etools .json is refused with a pointer to 5etools import and never sent", async () => {
    const user = userEvent.setup();
    const { api, seen } = watched();
    renderApp("/import", { api });
    await user.upload(await screen.findByLabelText("Choose files"), new File([FIVETOOLS], "spells-syn.json", { type: "application/json" }));
    expect(await screen.findByText(/"spells-syn.json" is 5etools data\..*Additional Content > 5etools import/)).toBeTruthy();
    expect(screen.queryByRole("list", { name: "Files to import" })).toBeNull();
    expect(seen).toHaveLength(0);
  });

  it("an Aurora file renamed to .txt is refused with a pointer to Custom content", async () => {
    const user = userEvent.setup();
    const { api, seen } = watched();
    renderApp("/import", { api });
    await user.upload(await screen.findByLabelText("Choose files"), new File([AURORA], "my class.txt", { type: "text/plain" }));
    expect(await screen.findByText(/"my class.txt" is Aurora content\..*Additional Content > Custom content/)).toBeTruthy();
    expect(seen).toHaveLength(0);
  });

  it("a Borealis pack .json is refused", async () => {
    const user = userEvent.setup();
    const { api, seen } = watched();
    renderApp("/import", { api });
    await user.upload(await screen.findByLabelText("Choose files"), new File([BOREALIS], "pack.json", { type: "application/json" }));
    expect(await screen.findByText(/"pack.json" is already a Borealis content pack/)).toBeTruthy();
    expect(seen).toHaveLength(0);
  });

  it("pasted Aurora XML is refused on Import and the AI import API is never called", async () => {
    const user = userEvent.setup();
    const { api, seen } = watched();
    renderApp("/import", { api });
    const box = await screen.findByLabelText("Paste text");
    await user.click(box);
    await user.paste(AURORA);
    await user.click(screen.getByRole("button", { name: "Import" }));
    expect(await screen.findByText(/The pasted text is Aurora content\./)).toBeTruthy();
    expect(seen).toHaveLength(0);
  });

  it("pasted 5etools JSON is refused", async () => {
    const user = userEvent.setup();
    const { api, seen } = watched();
    renderApp("/import", { api });
    await user.click(await screen.findByLabelText("Describe it"));
    await user.paste(FIVETOOLS_ENTITY);
    await user.click(screen.getByRole("button", { name: "Import" }));
    expect(await screen.findByText(/The pasted text is 5etools data\./)).toBeTruthy();
    expect(seen).toHaveLength(0);
  });

  it("a file dropped elsewhere in the app is checked before Import sends it", async () => {
    const user = userEvent.setup();
    const { api, seen } = watched();
    stashImportFiles([new File([AURORA_FRAGMENT], "notes.md", { type: "text/markdown" })]);
    renderApp("/import", { api });
    await user.click(await screen.findByRole("button", { name: "Import" }));
    expect(await screen.findByText(/"notes.md" is Aurora content\./)).toBeTruthy();
    expect(seen).toHaveLength(0);
  });

  it("ordinary text still imports", async () => {
    const user = userEvent.setup();
    const { api, seen } = watched();
    renderApp("/import", { api });
    await user.click(await screen.findByLabelText("Paste text"));
    await user.paste("Test Spark. Cantrip, evocation. A tiny spark of light.");
    await user.click(screen.getByRole("button", { name: "Import" }));
    await waitFor(() => expect(seen).toHaveLength(1));
  });
});
