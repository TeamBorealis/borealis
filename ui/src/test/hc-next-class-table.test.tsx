import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewForm } from "../api/contract";
import { classTableRows } from "../pages/homebrew/class/classTable";
import { RichTextEditor } from "../pages/homebrew/rich/RichTextEditor";
import type { DocJson } from "../pages/homebrew/rich/types";

// "Insert class table" (board HpbpM, spec 4.5, 7.1): the class description gets a table written from the LEVELS grid:
// level, proficiency bonus, the features at each level (an ability score improvement at each of its levels) and the
// class table's counters.

const child = (key: string, fields: Record<string, unknown>): HomebrewForm => ({ kind: "classFeature", key, fields, blocks: [], children: [] }) as unknown as HomebrewForm;

const ember: HomebrewForm = {
  kind: "class",
  fields: {
    name: "Emberwright",
    levels: [
      { child: "n1", level: 1 }, { child: "n2", level: 1 }, { child: "n3", level: 4 }, { child: "n4", level: 3 },
      { element: { id: "ID_X_FEATURE_EXTRA", name: "Extra Attack", type: "Class Feature" }, level: 5 },
    ],
    table: [
      { key: "proficiency_bonus", label: "Proficiency Bonus", values: Array(20).fill(2) },
      { key: "embers", label: "Embers", values: [2, 2, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 7, 7, 7, 7] },
      { key: "flare", label: "Flare Damage", values: ["1d4", "1d4", "1d4", "1d4", "1d6", 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
    ],
  },
  blocks: [],
  children: [
    child("n1", { name: "Kindle" }), child("n2", { name: "Spark Sense" }),
    child("n3", { name: "Ability Score Improvement", asiLevels: [4, 8] }), child("n4", { name: "Forge", subclassLabel: "Forge" }),
  ],
} as unknown as HomebrewForm;

describe("class table", () => {
  it("is written from the grid", () => {
    const rows = classTableRows(ember);
    expect(rows[0]).toEqual(["Level", "Proficiency Bonus", "Features", "Embers", "Flare Damage"]);
    expect(rows).toHaveLength(21);
    expect(rows[1]).toEqual(["1st", "+2", "Kindle, Spark Sense", "2", "1d4"]);
    expect(rows[2]).toEqual(["2nd", "+2", "—", "2", "1d4"]);
    expect(rows[3]?.slice(0, 3)).toEqual(["3rd", "+2", "Forge"]);
    expect(rows[4]?.slice(0, 3)).toEqual(["4th", "+2", "Ability Score Improvement"]);
    expect(rows[5]).toEqual(["5th", "+3", "Extra Attack", "4", "1d6"]);
    expect(rows[6]?.[4]).toBe("—");
    expect(rows[8]?.slice(0, 3)).toEqual(["8th", "+3", "Ability Score Improvement"]);
    expect(rows[20]?.slice(0, 2)).toEqual(["20th", "+6"]);
  });

  // Im8nQ: a 2014 caster's slots live in its Spellcasting feature (the caster type or a custom table), not in the class
  // table; the inserted table adds them as the SRD 5.1 class tables write them: cantrips known, spells known for a
  // known caster, then the slots of each slot level the class reaches (a pact caster: Spell Slots and Slot Level).
  const caster2014 = (fields: Record<string, unknown>): HomebrewForm => ({
    ...ember,
    fields: { ...ember.fields, edition: "dnd2014", table: [ember.fields.table as unknown[]][0]!.slice(1) },
    children: [...ember.children, child("n5", { name: "Spellcasting", casts: true, ...fields })],
  }) as unknown as HomebrewForm;
  const cantrips = { levels: [3, 3, 3, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5], statistic: false };

  it("adds a 2014 full caster's cantrips and spell slots", () => {
    const rows = classTableRows(caster2014({ casterType: "full", preparation: "prepared", cantrips }));
    expect(rows[0]).toEqual(["Level", "Proficiency Bonus", "Features", "Embers", "Flare Damage", "Cantrips Known", "1st", "2nd", "3rd", "4th", "5th", "6th", "7th", "8th", "9th"]);
    expect(rows[1]?.slice(5)).toEqual(["3", "2", "—", "—", "—", "—", "—", "—", "—", "—"]);
    expect(rows[5]?.slice(5)).toEqual(["4", "4", "3", "2", "—", "—", "—", "—", "—", "—"]);
    expect(rows[20]?.slice(5)).toEqual(["5", "4", "3", "3", "3", "3", "2", "2", "1", "1"]);
  });

  it("adds a 2014 half caster's spells known and the slot levels it reaches", () => {
    const known = { levels: [0, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11], statistic: false };
    const rows = classTableRows(caster2014({ casterType: "halfDown", preparation: "known", known }));
    expect(rows[0]?.slice(5)).toEqual(["Spells Known", "1st", "2nd", "3rd", "4th", "5th"]);
    expect(rows[1]?.slice(5)).toEqual(["—", "—", "—", "—", "—", "—"]);
    expect(rows[2]?.slice(5)).toEqual(["2", "2", "—", "—", "—", "—"]);
  });

  it("adds a 2014 pact caster's slots and their level", () => {
    const rows = classTableRows(caster2014({ casterType: "pact", preparation: "known", cantrips: { ...cantrips, levels: Array(20).fill(2) } }));
    expect(rows[0]?.slice(5)).toEqual(["Cantrips Known", "Spells Known", "Spell Slots", "Slot Level"]);
    expect(rows[1]?.slice(5)).toEqual(["2", "—", "1", "1st"]);
    expect(rows[11]?.slice(7)).toEqual(["3", "5th"]);
  });

  it("adds nothing for a class without spellcasting or one whose table holds its spellcasting (2024)", () => {
    expect(classTableRows(caster2014({ casts: false, casterType: "full" }))[0]).toHaveLength(5);
    const with2024 = { ...ember, children: [...ember.children, child("n5", { name: "Spellcasting", casts: true, casterType: "full" })],
      fields: { ...ember.fields, table: [...(ember.fields.table as unknown[]), { key: "slot_1", label: "1st", values: Array(20).fill(2) }] } } as unknown as HomebrewForm;
    expect(classTableRows(with2024)[0]).toEqual(["Level", "Proficiency Bonus", "Features", "Embers", "Flare Damage", "1st"]);
  });

  it("the description's Insert class table button adds it at the end", async () => {
    const onChange = vi.fn<(doc: DocJson, typing: boolean) => void>();
    const value: DocJson = { type: "doc", content: [{ type: "paragraph", content: [{ type: "text", text: "Fire folk." }] }] } as DocJson;
    render(<RichTextEditor id="d" label="Description" value={value} onChange={onChange} extras="class" classTable={() => classTableRows(ember)} />);
    const button = within(screen.getByRole("toolbar", { name: "Description formatting" })).getByRole("button", { name: "Insert class table" });
    expect(button.getAttribute("aria-disabled")).not.toBe("true");
    fireEvent.click(button);
    await waitFor(() => expect(onChange).toHaveBeenCalled());
    const [doc, typing] = onChange.mock.calls[onChange.mock.calls.length - 1]!;
    expect(typing).toBe(false);
    const blocks = doc.content ?? [];
    expect(blocks[0]?.type).toBe("paragraph");
    // the editor's JSON, read loosely (a table's rows and cells)
    type Loose = { type: string; text?: string; attrs?: { head?: boolean }; content?: Loose[] };
    const table = blocks[1] as unknown as Loose;
    expect(table.type).toBe("table");
    const cells = (row: number) => (table.content?.[row]?.content ?? []).map((c) => [c.type, (c.content ?? []).map((t) => t.text).join("")]);
    expect(cells(0)[0]).toEqual(["table_header", "Level"]);
    expect(table.content?.[0]?.attrs?.head).toBe(true);
    expect(cells(1)).toEqual([["table_cell", "1st"], ["table_cell", "+2"], ["table_cell", "Kindle, Spark Sense"], ["table_cell", "2"], ["table_cell", "1d4"]]);
    expect(table.content).toHaveLength(21);
  });
});
