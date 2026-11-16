import { describe, expect, it } from "vitest";
import type { HomebrewForm } from "../api/contract";
import { settleNote, type ShownNote } from "../pages/homebrew/statusNote";

// HC18 creator messages: what the kept status note becomes when the form on screen changes. Device flow 3 on the iOS
// simulator (lane O, 2026-11-16): "Starting equipment added." was kept for the class form, Save wrote "Saved 'Gloom
// Knight' to Ember Isles", and only then did the effect that saw the old note run (the saved element had reopened as a
// new form) and clear the line. Traced on the device: note written at 155,338 ms, the old note's clearing at 155,339 ms.

const formA = { kind: "class", fields: { name: "Gloom Knight" } } as unknown as HomebrewForm;
const formB = { kind: "class", fields: { name: "Gloom Knight" } } as unknown as HomebrewForm;
const kept: ShownNote = { text: "Starting equipment added.", count: true, form: formA };
const saved: ShownNote = { text: "Saved 'Gloom Knight' to Ember Isles", count: true };

describe("HC18 settleNote", () => {
  it("keeps a newer message written after the effect read the old one (Save right after a kept message)", () => {
    expect(settleNote(saved, kept, formB)).toBe(saved);
  });

  it("clears the note it saw once the form on screen is another one", () => {
    expect(settleNote(kept, kept, formB)).toBeNull();
  });

  it("keeps the note it saw while its form is on screen", () => {
    expect(settleNote(kept, kept, formA)).toBe(kept);
  });

  it("binds a new message to the form on screen when it first shows", () => {
    expect(settleNote(saved, saved, formB)).toEqual({ ...saved, form: formB });
  });

  it("with no element open, the message leaves the note (it is the plain line)", () => {
    expect(settleNote(saved, saved, null)).toBeNull();
  });
});
