import { describe, expect, it } from "vitest";
import type { PlayResource } from "../api/contract";
import { resourceMeta } from "../pages/play/PlayCards";

const pool = (over: Partial<PlayResource>): PlayResource => ({
  id: "free_cast:SYN_GRANTER:SYN_SPELL", name: "Synth Spell (free cast)", max: 1, current: 1, source: "Synth Trait",
  recharge: ["long_rest"], shortRestRegain: null, longRestRegainAll: true, ...over,
});

describe("free cast resource rows", () => {
  it("names the recharge period next to the source", () => {
    expect(resourceMeta(pool({}), null)).toBe("Synth Trait · per long rest");
    expect(resourceMeta(pool({ recharge: ["short_rest"] }), null)).toBe("Synth Trait · per short rest");
  });

  it("a free cast that never recharges reads as no recharge", () => {
    expect(resourceMeta(pool({ recharge: ["none"], longRestRegainAll: false }), null)).toBe("Synth Trait · no recharge");
  });

  it("leaves ordinary resources unchanged", () => {
    expect(resourceMeta(pool({ id: "rage", source: "Mock class" }), null)).toBe("Mock class");
    expect(resourceMeta(pool({ id: "wand", source: "Wand" }), "5")).toBe("Wand · max 5");
  });
});
