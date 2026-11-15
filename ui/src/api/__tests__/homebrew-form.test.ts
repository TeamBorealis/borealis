import { describe, expect, it } from "vitest";
import fs from "node:fs";
import path from "node:path";
import type {
  HomebrewBlock,
  HomebrewBlockValue,
  HomebrewCondition,
  HomebrewControl,
  HomebrewExprValue,
  HomebrewForm,
  HomebrewKindDescriptor,
  HomebrewMapsToValue,
  HomebrewSetterRow,
  HomebrewSheetValue,
} from "../types";
import type { DocJson } from "../../pages/homebrew/rich/types";
import { MOCK_BLOCK_CATALOG } from "../mock/homebrew-catalog";
import { blockShapeProblems } from "../mock/homebrew-shapes";

// WB-04 (and the WB-04/WB-05 join for blocks): the form the backend sends (Rust `borealis_authoring::form`) has the shapes the TS types declare. The Rust
// test `crates/borealis-authoring/tests/form_dto.rs` writes a sample form and three kind descriptors to the fixture;
// the sample is written here as a literal of the TS types, so the compiler checks it against the types and this test
// checks it against what Rust writes.
const fixture = JSON.parse(
  fs.readFileSync(path.resolve(process.cwd(), "src/api/__fixtures__/homebrew-form.json"), "utf8"),
) as { form: unknown; kinds: unknown[] };

const requirements: HomebrewExprValue<HomebrewCondition> = {
  "original": "[level:3]",
  "value": {
    "model": {
      "items": [
        {
          "node": "row",
          "row": {
            "min": 3,
            "row": "level"
          }
        }
      ],
      "mode": "all",
      "node": "group"
    },
    "shape": "model"
  }
};
const description: DocJson = {
  "content": [
    {
      "attrs": {
        "bare": false,
        "joined": false
      },
      "content": [
        {
          "text": "A ",
          "type": "text"
        },
        {
          "marks": [
            {
              "attrs": {
                "tag": "b"
              },
              "type": "bold"
            }
          ],
          "text": "wolf",
          "type": "text"
        },
        {
          "text": ".",
          "type": "text"
        }
      ],
      "type": "paragraph"
    }
  ],
  "type": "doc"
};
const sheet: HomebrewSheetValue = {
  "action": null,
  "alt": null,
  "display": null,
  "lines": [
    {
      "action": null,
      "level": 2,
      "text": "Bite {{level}}",
      "usage": null
    }
  ],
  "usage": "1/Long Rest"
};
const setters: HomebrewSetterRow[] = [
  {
    "attributes": {
      "unit": "ft"
    },
    "name": "speed",
    "value": "40"
  }
];
const mapsTo: HomebrewMapsToValue = {
  "dnd2014": {
    "id": "ID_WOLF",
    "missing": true,
    "name": "",
    "type": ""
  },
  "dnd2024": null,
  "kind": "approximate"
};

const statEquipped: HomebrewBlockValue<"equipped"> = {
  "original": "[armor:none]",
  "value": {
    "model": {
      "items": [
        {
          "node": "row",
          "row": {
            "row": "slot",
            "slot": "armor",
            "test": {
              "kind": "none"
            }
          }
        }
      ],
      "mode": "all",
      "node": "group"
    },
    "shape": "model"
  }
};
const grantId: HomebrewBlockValue<"reference"> = {
  "id": "ID_GL_TRAIT_PACK_TACTICS",
  "name": "Pack Tactics",
  "type": "Racial Trait"
};
const grantOnlyIf: HomebrewBlockValue<"condition"> = {
  "original": "[level:3]",
  "value": {
    "model": {
      "items": [
        {
          "node": "row",
          "row": {
            "min": 3,
            "row": "level"
          }
        }
      ],
      "mode": "all",
      "node": "group"
    },
    "shape": "model"
  }
};
const selectFrom: HomebrewBlockValue<"tagFilter"> = {
  "original": "Standard||Exotic",
  "value": {
    "model": {
      "root": {
        "items": [
          {
            "node": "row",
            "row": {
              "row": "tag",
              "tag": "Standard"
            }
          },
          {
            "node": "row",
            "row": {
              "row": "tag",
              "tag": "Exotic"
            }
          }
        ],
        "mode": "any",
        "node": "group"
      },
      "specific": false
    },
    "shape": "model"
  }
};
const listRows: HomebrewBlockValue<"rows"> = [
  {
    "id": 1,
    "text": "Grey"
  },
  {
    "id": 2,
    "text": "White"
  }
];
const maxFormula: HomebrewBlockValue<"formula"> = {
  "original": "max(1, wisdom:modifier)",
  "value": {
    "model": {
      "args": [
        {
          "term": "number",
          "value": 1
        },
        {
          "ability": "wisdom",
          "term": "ability"
        }
      ],
      "term": "max"
    },
    "shape": "model"
  }
};
const regain: HomebrewBlockValue<"map"> = {
  "short_rest": 1
};
const plus: HomebrewBlockValue<"formulas"> = [
  {
    "original": "dexterity:modifier",
    "value": {
      "model": {
        "ability": "dexterity",
        "term": "ability"
      },
      "shape": "model"
    }
  },
  {
    "original": "monk level",
    "value": {
      "model": {
        "class": "monk",
        "term": "classLevel"
      },
      "shape": "model"
    }
  }
];
const effects: HomebrewBlockValue<"blocks"> = [
  {
    "fields": {
      "ability": "strength",
      "roll": "ability_check"
    },
    "key": "e0",
    "kind": "roll_advantage",
    "sentence": "Advantage on Strength checks"
  },
  {
    "fields": {
      "name": "speed",
      "value": "10"
    },
    "key": "e1",
    "kind": "stat",
    "sentence": "Changes Walking speed: +10"
  }
];
const maxValue: HomebrewBlockValue<"variant"> = { type: "formula", expr: maxFormula };
const blocks: HomebrewBlock[] = [
  {
    "fields": {
      "equipped": statEquipped,
      "name": "speed",
      "value": "40"
    },
    "key": "b0",
    "kind": "stat",
    "preserved": [
      {
        "key": "minimum",
        "label": "A setting Borealis does not use (“minimum”)"
      }
    ],
    "sentence": "Changes Walking speed: +40, while wearing no armor"
  },
  {
    "fields": {
      "id": grantId,
      "level": 3,
      "requirements": grantOnlyIf,
      "type": "Racial Trait"
    },
    "key": "b1",
    "kind": "grant",
    "sentence": "From level 3: gives Pack Tactics, only if level 3 or higher"
  },
  {
    "fields": {
      "name": "Language (Wolf)",
      "supports": selectFrom,
      "type": "Language"
    },
    "key": "b2",
    "kind": "select",
    "sentence": "Lets the player choose: Language (Wolf), from Standard or Exotic"
  },
  {
    "fields": {
      "items": listRows,
      "name": "Coat (Wolf)",
      "type": "List"
    },
    "key": "b3",
    "kind": "list",
    "sentence": "Lets the player pick from a written list: Coat (Wolf)"
  },
  {
    "fields": {
      "id": "howl",
      "max": maxValue,
      "name": "Howl",
      "regain": regain
    },
    "key": "b4",
    "kind": "resource",
    "sentence": "Limited uses: Howl (the higher of 1 and Wisdom modifier)"
  },
  {
    "fields": {
      "damage_types": [
        "bludgeoning"
      ],
      "dice": "1d6",
      "plus": plus
    },
    "key": "b5",
    "kind": "damage_reduction",
    "sentence": "Reduces bludgeoning damage by 1d6 + Dexterity modifier + Monk level"
  },
  {
    "fields": {
      "activation": "bonus_action",
      "effects": effects,
      "id": "frenzy",
      "name": "Frenzy"
    },
    "key": "b6",
    "kind": "toggle",
    "sentence": "On/off state: Frenzy (2 effects)"
  },
  {
    "fields": {},
    "key": "b7",
    "kind": "unknown",
    "preserved": [
      {
        "key": "rule",
        "label": "A rule Borealis does not read (“howl”)"
      }
    ],
    "sentence": "Kept as is: a rule this creator does not edit"
  }
];

const form: HomebrewForm = {
  "blocks": blocks,
  "children": [],
  "fields": {
    "allowDuplicate": true,
    "compendium": false,
    "description": description,
    "edition": "dnd2024",
    "keywords": [
      "beast",
      "pet"
    ],
    "mapsTo": mapsTo,
    "name": "Wolf",
    "prerequisiteText": "Level 3",
    "requirements": requirements,
    "setters": setters,
    "sheet": sheet,
    "source": "Gloom",
    "summary": "A wolf.",
    "tags": [
      "Beast"
    ],
    "type": "Companion"
  },
  "key": "e",
  "kind": "other",
  "preserved": [
    {
      "key": "data.pack_size",
      "label": "Stored detail “pack size”"
    }
  ]
};

const CONTROLS: HomebrewControl[] = [
  "text", "multiline", "number", "combo", "check", "toggle", "tags", "chips", "reference", "references",
  "statistic", "condition", "tagFilter", "equipped", "formula", "richText", "sheet", "grid", "blocks", "children",
  "setters", "mapsTo",
];

describe("homebrew form shapes (WB-04)", () => {
  it("the sample form equals what the backend writes", () => {
    expect(form).toEqual(fixture.form);
  });

  it("kind descriptors have the declared shape", () => {
    const kinds = fixture.kinds as HomebrewKindDescriptor[];
    expect(kinds.map((k) => k.id)).toEqual(["language", "other", "append"]);
    for (const k of kinds) {
      const tabs = new Set(k.tabs.map((t) => t.id));
      for (const f of k.fields) {
        expect(CONTROLS).toContain(f.control);
        expect(tabs.has(f.tab)).toBe(true);
        expect(f.bindings.length).toBeGreaterThan(0);
        expect(typeof f.required).toBe("boolean");
      }
    }
    const source = kinds[0]?.fields.find((f) => f.key === "source");
    expect(source?.readOnly).toBe(true);
  });

  it("blocks are catalog blocks and their fields are the dialog's fields", () => {
    const catalog = new Map(MOCK_BLOCK_CATALOG.map((b) => [b.kind, b]));
    const check = (b: HomebrewBlock) => {
      const spec = catalog.get(b.kind);
      if (!spec) {
        // a rule kept whole: no fields, one kept part
        expect(Object.keys(b.fields)).toEqual([]);
        expect(b.preserved?.length).toBe(1);
        return;
      }
      const keys = new Set(spec.fields.map((f) => f.key));
      if (spec.levelGated) keys.add("level");
      if (spec.onlyIf) keys.add("requirements");
      for (const k of Object.keys(b.fields)) expect(keys.has(k)).toBe(true);
      expect(b.sentence.length).toBeGreaterThan(0);
    };
    for (const b of blocks) check(b);
    for (const e of effects) {
      expect(e.preserved).toBeUndefined();
      check(e);
    }
    expect(blocks.map((b) => b.kind)).toContain("list");
  });

  it("the mock reads every block of the backend's sample form as the backend does (hc-wave3-integrate)", () => {
    const sample = fixture.form as HomebrewForm;
    expect(sample.blocks.flatMap((b, i) => blockShapeProblems(b, i, MOCK_BLOCK_CATALOG, null))).toEqual([]);
  });

  it("the mock refuses the raw-codec shapes the backend no longer reads", () => {
    const at = (b: HomebrewBlock) => blockShapeProblems(b, 0, MOCK_BLOCK_CATALOG, null).map((p) => p.path);
    // a grant's element as a bare id (now a `HomebrewRef`)
    expect(at({ key: "a", kind: "grant", sentence: "", fields: { id: "ID_X", type: "Feat" } })).toEqual(["blocks.0.fields.id"]);
    // a written list's row numbers as text (the engine reads whole numbers)
    expect(at({ key: "b", kind: "list", sentence: "", fields: { type: "List", name: "Coat", items: [{ id: "1", text: "Grey" }] } })).toEqual(["blocks.0.fields.items.0.id"]);
    // a toggle's effects as stored rules (now nested blocks)
    expect(at({ key: "c", kind: "toggle", sentence: "", fields: { id: "f", name: "F", effects: [{ kind: "stat", name: "speed", value: "10" }] } })).toEqual(["blocks.0.fields.effects.0"]);
    // a formula as text (now a builder value), the attack bonus as a formula (now a number)
    expect(at({ key: "d", kind: "attack_bonus", sentence: "", fields: { value: "2" } })).toEqual(["blocks.0.fields.value"]);
    // a field the block does not have
    expect(at({ key: "e", kind: "sense", sentence: "", fields: { sense: "darkvision", merge: "x" } })).toEqual(["blocks.0.fields.merge"]);
  });
});
