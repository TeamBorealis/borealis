import { describe, expect, it } from "vitest";
import fs from "node:fs";
import path from "node:path";
import type { HomebrewForm } from "../types";
import { featPrerequisite } from "../mock/homebrew-feat";

// Hm2aU: the web mock words a feat draft's prerequisite as the backend does. The Rust test
// `crates/borealis-authoring/tests/mock_feat_prerequisites.rs` writes new feat forms with the sentence the real feat
// codec writes for each; the mock reads the same forms here.
interface Fixture {
  names: Record<string, string>;
  cases: { case: string; form: HomebrewForm; prerequisite: string }[];
}

const fixture = JSON.parse(
  fs.readFileSync(path.resolve(process.cwd(), "src/api/__fixtures__/homebrew-feat-prerequisites.json"), "utf8"),
) as Fixture;

describe("the mock's feat prerequisite sentence", () => {
  it("has cases", () => expect(fixture.cases.length).toBeGreaterThan(10));
  it.each(fixture.cases.map((c) => [c.case, c] as const))("%s: as the backend writes it", (_, c) => {
    const nameOf = (id: string) => fixture.names[id] ?? null;
    // the backend writes "" where no sentence can be made; the mock answers null
    expect(featPrerequisite(c.form, nameOf) ?? "").toBe(c.prerequisite);
  });
});
