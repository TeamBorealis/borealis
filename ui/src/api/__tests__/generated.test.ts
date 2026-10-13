import { describe, expect, it } from "vitest";
import fs from "node:fs";
import path from "node:path";
import { generate } from "../../../scripts/gen-contract.mjs";

describe("generated contract", () => {
  it("src/api/generated/contract.json is up to date with contract.ts", () => {
    const committed = fs.readFileSync(path.resolve(process.cwd(), "src/api/generated/contract.json"), "utf8");
    expect(generate()).toBe(committed);
  });
});
