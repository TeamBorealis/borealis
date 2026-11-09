import { describe, expect, it } from "vitest";
import type { PlayResource } from "../api/contract";
import { rechargeText, resourceMeta } from "../pages/play/PlayCards";

const pool = (over: Partial<PlayResource>): PlayResource => ({
  id: "free_cast:SYN_GRANTER:SYN_SPELL", name: "Synth Spell (free cast)", max: 1, current: 1, source: "Synth Trait",
  recharge: ["long_rest"], shortRestRegain: null, longRestRegainAll: true, ...over,
});

describe("free cast resource rows", () => {
  it("names how the use comes back next to the source", () => {
    expect(resourceMeta(pool({}), null)).toBe("Synth Trait · All back on a Long Rest");
    expect(resourceMeta(pool({ recharge: ["short_rest"], shortRestRegain: 1 }), null)).toBe("Synth Trait · All back on a Short or Long Rest");
  });

  it("a free cast that never recharges reads as no recharge", () => {
    expect(resourceMeta(pool({ recharge: ["none"], longRestRegainAll: false }), null)).toBe("Synth Trait · No recharge");
  });
});

// B-141 (tester video C7): the grey text said the name again ("Rage · Rage"), and the detail said "Resets on Short Rest /
// Long Rest" although a Short Rest gives back one Rage use.
describe("resource rows say how uses come back", () => {
  const rage = pool({ id: "rage", name: "Rage", source: "Rage", max: 2, current: 2, recharge: ["short_rest", "long_rest"], shortRestRegain: 1 });

  it("resource meta shows the recharge rule, not a repeated name", () => {
    expect(resourceMeta(rage, null)).toBe("1 back on a Short Rest, all on a Long Rest");
    expect(resourceMeta(pool({ id: "breath", name: "Breath Weapon", source: "Breath Weapon", max: 2 }), null)).toBe("All back on a Long Rest");
  });

  it("keeps a source that differs from the name, and the rolled maximum", () => {
    expect(resourceMeta({ ...rage, source: "Synth Class" }, null)).toBe("Synth Class · 1 back on a Short Rest, all on a Long Rest");
    expect(resourceMeta(pool({ id: "wand", name: "Charges", source: "Wand", recharge: ["dawn"], longRestRegainAll: false }), "5")).toBe("Wand · Recharges at dawn · max 5");
  });

  it("covers every recharge the content uses", () => {
    expect(rechargeText(pool({ max: 3, recharge: ["short_rest", "long_rest"], shortRestRegain: 3 }))).toBe("All back on a Short or Long Rest");
    expect(rechargeText(pool({ recharge: ["turn_start"], longRestRegainAll: false }))).toBe("Recharges at the start of your turn");
    expect(rechargeText(pool({ recharge: ["initiative", "short_rest", "long_rest"], shortRestRegain: 1 }))).toBe("All back on a Short or Long Rest, or when you roll Initiative");
    expect(rechargeText(pool({ recharge: [], longRestRegainAll: true }))).toBe("All back on a Long Rest");
  });
});
