import { describe, expect, it } from "vitest";
import fs from "node:fs";
import path from "node:path";
import { MockApi } from "../mock";

// UI-vs-engine (B-22), MockApi side: every snapshot the mock returns must match the shape in the
// generated contract (the same file the Rust test `src-tauri/tests/snapshot_contract.rs` validates the
// real DTOs against), with no missing required field and no undeclared extra field.
type Shape = { k: string; name?: string; t?: Shape; props?: Record<string, { opt: boolean; t: Shape }>; open?: boolean; variants?: Shape[] };
const contract = JSON.parse(fs.readFileSync(path.resolve(process.cwd(), "src/api/generated/contract.json"), "utf8")) as { defs: Record<string, Shape> };

function check(shape: Shape, v: unknown, at: string, errs: string[]): void {
  switch (shape.k) {
    case "ref":
      return check(contract.defs[shape.name!]!, v, at, errs);
    case "object": {
      if (v === null || v === undefined) return;
      if (typeof v !== "object" || Array.isArray(v)) return void errs.push(`${at}: expected object`);
      const props = shape.props ?? {};
      for (const [k, x] of Object.entries(v)) {
        if (x === undefined) continue;
        if (props[k]) check(props[k]!.t, x, `${at}.${k}`, errs);
        else if (!shape.open) errs.push(`${at}: mock sends \`${k}\` which the contract does not declare`);
      }
      for (const [k, p] of Object.entries(props)) if (!p.opt && !(k in v)) errs.push(`${at}: contract requires \`${k}\``);
      return;
    }
    case "array":
      if (Array.isArray(v)) v.forEach((x, i) => check(shape.t!, x, `${at}[${i}]`, errs));
      else if (v != null) errs.push(`${at}: expected array`);
      return;
    case "record":
      if (v && typeof v === "object") for (const [k, x] of Object.entries(v)) check(shape.t!, x, `${at}.${k}`, errs);
      return;
    case "prim":
      if (v && typeof v === "object") errs.push(`${at}: expected a scalar`);
      return;
    case "union": {
      if (v == null) return;
      const tries = shape.variants!.map((s) => {
        const e: string[] = [];
        check(s, v, at, e);
        return e;
      });
      if (!tries.some((e) => e.length === 0)) errs.push(...tries.sort((a, b) => a.length - b.length)[0]!);
    }
  }
}

describe("MockApi snapshots match the contract", () => {
  it("every seeded character's snapshot has exactly the declared fields", async () => {
    const api = new MockApi();
    const errs: string[] = [];
    const list = await api.listCharacters();
    expect(list.length).toBeGreaterThan(1);
    for (const c of list) check({ k: "ref", name: "CharacterSnapshot" }, await api.getSnapshot({ characterId: c.id }), c.name, errs);
    expect(errs.slice(0, 20)).toEqual([]);
  });

  it("the checker flags a missing and an extra field", () => {
    const errs: string[] = [];
    check({ k: "object", props: { a: { opt: false, t: { k: "prim" } } } }, { b: 1 }, "x", errs);
    expect(errs).toHaveLength(2);
  });
});
