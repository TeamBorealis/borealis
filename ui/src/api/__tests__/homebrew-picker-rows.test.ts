import { describe, expect, it } from "vitest";
import fs from "node:fs";
import path from "node:path";
import type { ElementSummary, HomebrewPickerFilters, HomebrewPickerTarget } from "../types";
import { referenceRows } from "../mock/homebrew-check";

// Hlw0Y: the web mock's picker rows pass the same elements as the backend's (`homebrew_refs::passes`, its order). The
// Rust test `the_mock_fixture_holds_the_backend_s_picker_rows` (src-tauri `homebrew_refs.rs`) writes loaded elements
// (their summaries and tags) and the rows the backend answers for each target, query and filters; the mock answers
// the same cases here.
interface Fixture {
  loaded: ElementSummary[];
  cases: { case: string; target: HomebrewPickerTarget; query: string; filters: HomebrewPickerFilters; ids: string[] }[];
}

const fixture = JSON.parse(
  fs.readFileSync(path.resolve(process.cwd(), "src/api/__fixtures__/homebrew-picker-rows.json"), "utf8"),
) as Fixture;

describe("the mock's picker rows", () => {
  it("has cases", () => expect(fixture.cases.length).toBeGreaterThan(20));
  it.each(fixture.cases.map((c) => [c.case, c] as const))("%s: the backend's rows", (_, c) => {
    const page = referenceRows(fixture.loaded, [], c.target, c.query, { offset: 0, limit: 100 }, c.filters);
    expect(page.items.map((r) => r.id)).toEqual(c.ids);
    expect(page.total).toBe(c.ids.length);
  });
});
