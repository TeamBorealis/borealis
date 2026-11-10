import { describe, expect, it } from "vitest";
import fs from "node:fs";
import path from "node:path";
import type {
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

// WB-04: the form the backend sends (Rust `borealis_authoring::form`) has the shapes the TS types declare. The Rust
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

const form: HomebrewForm = {
  "blocks": [
    {
      "fields": {
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
      "sentence": "Rule: stat"
    }
  ],
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
});
