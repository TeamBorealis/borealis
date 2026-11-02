import { describe, expect, it } from "vitest";
import {
  DiceError, critical, damageDice, formatNotation, makeCryptoDie, parseNotation, rollD20, rollExpr, seededDie, sequenceDie, tryParseNotation,
} from "../lib/dice";

// GitHub #1 / #14: the dice behind every roll on the Play tab. A notation parser, crit doubling, advantage and
// disadvantage, an injectable die for tests and an unbiased crypto die in the app.

describe("parseNotation", () => {
  it("reads single dice, sums of dice and flat modifiers", () => {
    // Arrange / Act / Assert
    expect(formatNotation(parseNotation("1d8+3"))).toBe("1d8+3");
    expect(formatNotation(parseNotation("2d6"))).toBe("2d6");
    expect(formatNotation(parseNotation("1d4+1d6+2"))).toBe("1d4+1d6+2");
    expect(formatNotation(parseNotation("d20"))).toBe("1d20");
    expect(formatNotation(parseNotation(" 1D10 - 1 "))).toBe("1d10-1");
    expect(formatNotation(parseNotation("-1+1d4"))).toBe("-1+1d4");
    expect(formatNotation(parseNotation("5"))).toBe("5");
  });

  it("keeps the terms in order with their signs", () => {
    const expr = parseNotation("2d6-1d4+3");
    expect(expr.terms).toEqual([
      { kind: "dice", count: 2, sides: 6, sign: 1 },
      { kind: "dice", count: 1, sides: 4, sign: -1 },
      { kind: "flat", value: 3 },
    ]);
  });

  it.each([
    ["", "empty"],
    ["   ", "empty"],
    ["abc", "letters"],
    ["1d", "no sides"],
    ["0d6", "zero dice"],
    ["1d0", "zero sides"],
    ["1d6++2", "double operator"],
    ["1d6+", "trailing operator"],
    ["+", "operator only"],
    ["1d6 2", "missing operator"],
    ["1d6*2", "unknown operator"],
    ["101d6", "too many dice"],
    ["1d1001", "too many sides"],
    ["1d6+1.5", "a decimal"],
    [`${"1+".repeat(40)}1`, "too many terms"],
    ["9".repeat(12), "too large"],
  ])("refuses %j (%s)", (text) => {
    expect(() => parseNotation(text)).toThrow(DiceError);
    expect(tryParseNotation(text)).toBeNull();
  });
});

describe("damageDice", () => {
  it("takes the dice out of a damage text and drops the damage types", () => {
    expect(formatNotation(damageDice("1d8+3 slashing")!)).toBe("1d8+3");
    expect(formatNotation(damageDice("1d8+3 slashing + 1d6 fire")!)).toBe("1d8+3+1d6");
    expect(formatNotation(damageDice("2d10 Fire")!)).toBe("2d10");
    expect(formatNotation(damageDice("1d4-1 piercing")!)).toBe("1d4-1");
    expect(formatNotation(damageDice("4 bludgeoning")!)).toBe("4");
  });

  it("stops at text that is not part of the sum, and finds nothing in a save DC or plain words", () => {
    expect(formatNotation(damageDice("1d8+2 slashing, 2d6 on a hit while raging")!)).toBe("1d8+2");
    expect(damageDice("DC 13 DEX")).toBeNull();
    expect(damageDice("see notes")).toBeNull();
    expect(damageDice("")).toBeNull();
  });
});

describe("critical", () => {
  it("doubles the dice and never the modifiers", () => {
    expect(formatNotation(critical(parseNotation("1d8+3")))).toBe("2d8+3");
    expect(formatNotation(critical(parseNotation("2d6+1d4-1")))).toBe("4d6+2d4-1");
    expect(formatNotation(critical(parseNotation("4")))).toBe("4");
  });

  it("does not change the expression it was given", () => {
    const expr = parseNotation("1d8+3");
    critical(expr);
    expect(formatNotation(expr)).toBe("1d8+3");
  });
});

describe("rollExpr", () => {
  it("sums every die and modifier and writes the breakdown", () => {
    const r = rollExpr(parseNotation("2d6+1d4+2"), sequenceDie([3, 4, 2]));
    expect(r.total).toBe(11);
    expect(r.breakdown).toBe("2d6 [3, 4] +1d4 [2] +2 = 11");
    expect(r.terms.map((t) => t.rolls)).toEqual([[3, 4], [2], []]);
  });

  it("subtracts negative terms", () => {
    expect(rollExpr(parseNotation("1d4-1"), sequenceDie([1])).breakdown).toBe("1d4 [1] -1 = 0");
    expect(rollExpr(parseNotation("1d6-1d4"), sequenceDie([5, 2])).total).toBe(3);
  });

  it("counts dice under the floor as the floor and says so", () => {
    const r = rollExpr(parseNotation("2d6+3"), sequenceDie([1, 5]), { floor: 3 });
    expect(r.total).toBe(11);
    expect(r.breakdown).toBe("2d6 [3, 5] +3 = 11, dice below 3 count as 3");
  });

  it("marks a critical roll", () => {
    const r = rollExpr(critical(parseNotation("1d8+3")), sequenceDie([5, 3]), { label: "critical" });
    expect(r.breakdown).toBe("2d8 [5, 3] +3 = 11, critical");
  });
});

describe("rollD20", () => {
  it("rolls one d20 and adds the modifier", () => {
    const r = rollD20(5, "normal", sequenceDie([14]));
    expect(r.total).toBe(19);
    expect(r.natural).toBe(14);
    expect(r.breakdown).toBe("d20 [14] +5 = 19");
  });

  it("keeps the higher die with advantage and the lower with disadvantage", () => {
    expect(rollD20(5, "advantage", sequenceDie([14, 7])).breakdown).toBe("d20 [14, 7] +5 = 19, advantage");
    expect(rollD20(5, "disadvantage", sequenceDie([14, 7])).breakdown).toBe("d20 [14, 7] +5 = 12, disadvantage");
    expect(rollD20(-1, "disadvantage", sequenceDie([3, 18])).total).toBe(2);
  });

  it("reports a natural 20 and 1, and a critical hit from the crit range", () => {
    const twenty = rollD20(2, "normal", sequenceDie([20]));
    expect(twenty.isNat20).toBe(true);
    expect(twenty.isCrit).toBe(true);
    expect(twenty.breakdown).toBe("d20 [20] +2 = 22, natural 20");
    expect(rollD20(2, "normal", sequenceDie([1])).isNat1).toBe(true);
    expect(rollD20(2, "normal", sequenceDie([19]), { critRange: 19 }).isCrit).toBe(true);
    expect(rollD20(2, "normal", sequenceDie([19])).isCrit).toBe(false);
    // the kept die decides: disadvantage with a 20 and a 4 is no crit
    expect(rollD20(2, "disadvantage", sequenceDie([20, 4])).isCrit).toBe(false);
  });

  it("writes zero and negative modifiers with their sign", () => {
    expect(rollD20(0, "normal", sequenceDie([9])).breakdown).toBe("d20 [9] +0 = 9");
    expect(rollD20(-2, "normal", sequenceDie([9])).breakdown).toBe("d20 [9] -2 = 7");
  });
});

describe("distribution (seeded)", () => {
  it("a d6 lands on each face about a sixth of the time", () => {
    const die = seededDie(12345);
    const counts = [0, 0, 0, 0, 0, 0];
    const n = 60000;
    for (let i = 0; i < n; i++) counts[die(6) - 1]! += 1;
    for (const c of counts) expect(Math.abs(c - n / 6)).toBeLessThan(n / 6 * 0.05);
  });

  it("advantage averages about 13.8 and disadvantage about 7.2", () => {
    const die = seededDie(7);
    const n = 40000;
    const mean = (mode: "advantage" | "disadvantage") => {
      let sum = 0;
      for (let i = 0; i < n; i++) sum += rollD20(0, mode, die).total;
      return sum / n;
    };
    expect(mean("advantage")).toBeCloseTo(13.82, 0);
    expect(mean("disadvantage")).toBeCloseTo(7.18, 0);
  });

  it("the same seed gives the same rolls", () => {
    const a = seededDie(99);
    const b = seededDie(99);
    expect(Array.from({ length: 10 }, () => a(20))).toEqual(Array.from({ length: 10 }, () => b(20)));
  });
});

describe("makeCryptoDie", () => {
  it("throws away values that would bias the result and maps the rest to 1..sides", () => {
    // 2^32 = 4294967296; for a d6 every value at or above 4294967296 - (4294967296 % 6) = 4294967292 is rejected
    const values = [4294967295, 4294967292, 7];
    const die = makeCryptoDie((buf) => { buf[0] = values.shift()!; });
    expect(die(6)).toBe(2);
    expect(values).toEqual([]);
  });

  it("always lands inside the die", () => {
    const die = makeCryptoDie((buf) => crypto.getRandomValues(buf));
    for (let i = 0; i < 2000; i++) {
      const v = die(20);
      expect(v).toBeGreaterThanOrEqual(1);
      expect(v).toBeLessThanOrEqual(20);
    }
  });
});
