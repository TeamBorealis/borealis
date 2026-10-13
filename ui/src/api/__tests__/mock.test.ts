import { describe, expect, it } from "vitest";
import { MockApi } from "../mock";
import { COMMANDS } from "../contract";

describe("MockApi smoke", () => {
  it("lists seeded characters with builds", async () => {
    const api = new MockApi();
    const list = await api.listCharacters();
    expect(list.map((c) => c.build)).toEqual(expect.arrayContaining(["Dwarf Fighter 3", "Elf Wizard 2"]));
    expect(Object.keys(COMMANDS).length).toBeGreaterThan(60);
  });
  it("derives sensible stats for the sample fighter", async () => {
    const api = new MockApi();
    const [fighter] = (await api.listCharacters()).filter((c) => c.name.startsWith("Brannoc"));
    const s = await api.getSnapshot({ characterId: fighter!.id });
    expect(s.stats.armorClass).toBe(19);
    expect(s.stats.hpMax).toBeGreaterThan(20);
    expect(s.progress.remaining).toBe(0);
  });
  it("explains a statistic with a minimal trace that sums to the snapshot value", async () => {
    const api = new MockApi();
    const [fighter] = (await api.listCharacters()).filter((c) => c.name.startsWith("Brannoc"));
    const trace = await api.explainStat({ characterId: fighter!.id, stat: "AC" });
    expect(trace.stat).toBe("ac");
    expect(trace.value).toBe(19);
    expect(trace.steps.filter((x) => x.included).reduce((n, x) => n + x.value, 0)).toBe(trace.value);
    await expect(api.explainStat({ characterId: fighter!.id, stat: " " })).rejects.toMatchObject({ code: "invalid_argument" });
  });
});
