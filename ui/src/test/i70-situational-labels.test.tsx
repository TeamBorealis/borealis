import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { PlayAdvantage, PlayState } from "../api/contract";
import { SituationalCard } from "../pages/play/TraitsPanel";

// GitHub #70: the Play tab's Situational list printed the host's roll kind as is ("saving_throw Advantage on saving
// throws against being Frightened"). Each row now leads with a plain label; a kind the app doesn't know reads as words.
const play = (advantages: PlayAdvantage[]) =>
  ({ advantages, notes: [], initiativeAdvantage: false }) as unknown as PlayState;
const adv = (roll: string, text: string): PlayAdvantage => ({ roll, text, source: "Brave", active: true });

describe("Situational advantage labels", () => {
  it("shows a plain label for the host's roll kinds, never the raw key", () => {
    render(<SituationalCard play={play([
      adv("saving_throw", "Advantage on saving throws against being Frightened (to avoid or end the condition)"),
      adv("ability_check", "Advantage on Stealth checks"),
      adv("death_save", "Advantage on Death Saving Throws"),
      adv("spell_attack", "Advantage on spell attack rolls"),
      adv("initiative", "Advantage on initiative rolls"),
    ])} />);
    const list = screen.getByRole("list", { name: "Situational" });
    const text = list.textContent ?? "";
    expect(text).not.toMatch(/_/);
    for (const label of ["Saving throws", "Ability checks", "Death saves", "Spell attacks", "Initiative"]) {
      expect(within(list).getByText(label)).toBeTruthy();
    }
  });

  it("reads an unknown roll kind as words", () => {
    render(<SituationalCard play={play([adv("attack_roll", "Advantage on attack rolls")])} />);
    const list = screen.getByRole("list", { name: "Situational" });
    expect(within(list).getByText("Attack roll")).toBeTruthy();
    expect(list.textContent).not.toMatch(/_/);
  });
});
