import { describe, expect, it } from "vitest";
import fs from "node:fs";
import path from "node:path";
import type {
  HomebrewBuilderProblem,
  HomebrewCondition,
  HomebrewConditionRow,
  HomebrewEquipped,
  HomebrewEquippedRow,
  HomebrewExprNode,
  HomebrewExprValue,
  HomebrewFormulaDialect,
  HomebrewFormulaNode,
  HomebrewTagFilter,
  HomebrewTagRow,
} from "../types";

// WB-02: the builder states the backend sends (Rust `borealis_authoring::expr_build`) have the shapes the TS types
// declare. The Rust test `crates/borealis-authoring/tests/expr_dto.rs` writes one sample of every variant to the
// fixture; the same samples are written here as literals of the TS types, so the compiler checks them against the
// types and this test checks them against what Rust writes.
const fixture: unknown = JSON.parse(
  fs.readFileSync(path.resolve(process.cwd(), "src/api/__fixtures__/homebrew-expr-builders.json"), "utf8"),
);

const row = <R>(r: R): HomebrewExprNode<R> => ({ node: "row", row: r });
const all = <R>(items: HomebrewExprNode<R>[]): HomebrewExprNode<R> => ({ node: "group", mode: "all", items });
const num = (value: number): HomebrewFormulaNode => ({ term: "number", value });

const condition: HomebrewExprValue<HomebrewCondition> = {
  original:
    "ID_AA_BB&&!ID_CC_DD&&[str:13]&&[character:4]&&[level:3]&&[warlock:2]&&[feature:Rage]&&[spellcasting:1]" +
    "&&[type:Spell]&&[innate speed:fly:1]&&([ranger:archetype:hunter]||!([wis:13]||[int:13]))",
  value: {
    shape: "model",
    model: all<HomebrewConditionRow>([
      row({ row: "has", id: "ID_AA_BB" }),
      row({ row: "lacks", id: "ID_CC_DD" }),
      row({ row: "ability", ability: "strength", min: 13 }),
      row({ row: "characterLevel", min: 4 }),
      row({ row: "level", min: 3 }),
      row({ row: "classLevel", class: "warlock", min: 2 }),
      row({ row: "feature", name: "Rage" }),
      row({ row: "spellcasting", level: 1 }),
      row({ row: "type", value: "Spell" }),
      row({ row: "statistic", stat: "innate speed:fly", min: 1 }),
      {
        node: "group",
        mode: "any",
        items: [
          row({ row: "kept", text: "[ranger:archetype:hunter]" }),
          {
            node: "group",
            mode: "none",
            items: [
              row({ row: "ability", ability: "wisdom", min: 13 }),
              row({ row: "ability", ability: "intelligence", min: 13 }),
            ],
          },
        ],
      },
    ]),
  },
};

const conditionKept: HomebrewExprValue<HomebrewCondition> = {
  original: "((",
  value: { shape: "kept", text: "((", reason: "Borealis can't read this condition (missing operand)." },
};

const tagFilter: HomebrewExprValue<HomebrewTagFilter> = {
  original: "$(spellcasting:list),(1||2)||$(spellcasting:slots),$(param:Spell List)||Exotic||ID_AA_BB||$(custom)",
  value: {
    shape: "model",
    model: {
      specific: false,
      root: {
        node: "group",
        mode: "any",
        items: [
          all<HomebrewTagRow>([
            row({ row: "classList" }),
            { node: "group", mode: "any", items: [row({ row: "tag", tag: "1" }), row({ row: "tag", tag: "2" })] },
          ]),
          all<HomebrewTagRow>([row({ row: "castableLevel" }), row({ row: "choice", name: "Spell List" })]),
          row({ row: "tag", tag: "Exotic" }),
          row({ row: "element", id: "ID_AA_BB" }),
          row({ row: "kept", text: "$(custom)" }),
        ],
      },
    },
  },
};

const tagFilterSpecific: HomebrewExprValue<HomebrewTagFilter> = {
  value: {
    shape: "model",
    model: {
      specific: true,
      root: {
        node: "group",
        mode: "any",
        items: [row({ row: "element", id: "ID_AA_BB" }), row({ row: "element", id: "ID_CC_DD" })],
      },
    },
  },
};

const equipped: HomebrewExprValue<HomebrewEquipped> = {
  original:
    "[armor:light]&&[armor:medium]&&[armor:heavy]&&[armor:none]&&[shield:any]&&[main:weapon]" +
    "&&[main:versatile]&&[offhand:id_x_dagger]&&[attunement:id_x_ring]&&[item:id_x_cloak]&&[pet:any]",
  value: {
    shape: "model",
    model: all<HomebrewEquippedRow>([
      row({ row: "slot", slot: "armor", test: { kind: "light" } }),
      row({ row: "slot", slot: "armor", test: { kind: "medium" } }),
      row({ row: "slot", slot: "armor", test: { kind: "heavy" } }),
      row({ row: "slot", slot: "armor", test: { kind: "none" } }),
      row({ row: "slot", slot: "shield", test: { kind: "any" } }),
      row({ row: "slot", slot: "mainHand", test: { kind: "weapon" } }),
      row({ row: "slot", slot: "mainHand", test: { kind: "versatile" } }),
      row({ row: "slot", slot: "offHand", test: { kind: "item", item: "id_x_dagger" } }),
      row({ row: "attuned", item: "id_x_ring" }),
      row({ row: "wearing", item: "id_x_cloak" }),
      row({ row: "kept", text: "[pet:any]" }),
    ]),
  },
};

const formula: HomebrewExprValue<HomebrewFormulaNode> = {
  original: "max(1, wisdom:modifier) + ceil(cleric level / 2) - floor(proficiency) * min(3, 4) / 2",
  value: {
    shape: "model",
    model: {
      term: "binary",
      op: "sub",
      left: {
        term: "binary",
        op: "add",
        left: { term: "max", args: [num(1), { term: "ability", ability: "wisdom" }] },
        right: { term: "ceilDiv", num: { term: "classLevel", class: "cleric" }, den: num(2) },
      },
      right: {
        term: "binary",
        op: "div",
        left: {
          term: "binary",
          op: "mul",
          left: { term: "floor", arg: { term: "proficiency" } },
          right: { term: "min", args: [num(3), num(4)] },
        },
        right: num(2),
      },
    },
  },
};

const saveDc: HomebrewExprValue<HomebrewFormulaNode> = {
  original: "spell save DC (Cleric)",
  value: { shape: "model", model: { term: "classSaveDc", class: "Cleric" } },
};

const value: HomebrewExprValue<HomebrewFormulaNode> = {
  original: "proficiency:half",
  value: { shape: "model", model: { term: "halfProficiency" } },
};

const problems: HomebrewBuilderProblem[] = [{ path: "items.0", message: "Give the feature's name." }];

const dialects: HomebrewFormulaDialect[] = ["formula", "saveDc", "breathDc", "noteDc", "value", "valueOrClassLevel"];

describe("homebrew expression builder shapes (WB-02)", () => {
  it("match what the backend writes, variant for variant", () => {
    expect({
      condition,
      conditionKept,
      tagFilter,
      tagFilterSpecific,
      equipped,
      formula,
      saveDc,
      value,
      problems,
      dialects,
    }).toEqual(fixture);
  });
});
