import { screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { BuilderTab, CharacterSnapshot } from "../api/contract";
import { openPicksFor } from "../components/shell/tabBadges";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { renderApp, seededId } from "./utils";

// Tester B13 (B-136): the phone Build badge counted picks that live on Magic, so the tester looked for them on Build.
const byTab = (counts: Partial<Record<BuilderTab, number>>): Record<BuilderTab, number> => ({
  race: 0, class: 0, background: 0, abilities: 0, feats: 0, languages: 0, proficiencies: 0, companion: 0, spells: 0, ...counts,
});
const snap = (counts: Partial<Record<BuilderTab, number>>): CharacterSnapshot => {
  const tabs = byTab(counts);
  const remaining = Object.values(tabs).reduce((a, b) => a + b, 0);
  return { progress: { total: remaining, remaining, nextSlotId: null, byTab: tabs }, play: { equipmentPackages: [] } } as unknown as CharacterSnapshot;
};

/** Snapshots with one open pick on Feats and three on Magic (the tester's Magic Initiate state). */
class OpenPicksApi extends MockApi {
  override async loadCharacter(args: { characterId: string }): Promise<CharacterSnapshot> {
    return withOpenPicks(await super.loadCharacter(args));
  }
  override async getSnapshot(args: { characterId: string }): Promise<CharacterSnapshot> {
    return withOpenPicks(await super.getSnapshot(args));
  }
}
const withOpenPicks = (s: CharacterSnapshot): CharacterSnapshot =>
  ({ ...s, progress: { ...s.progress, remaining: 4, byTab: byTab({ feats: 1, spells: 3 }) } });

/** jsdom has no matchMedia: answer the narrow-shell query as a phone screen would, so the bottom bar renders. */
function narrowScreen(): () => void {
  const original = window.matchMedia;
  window.matchMedia = ((q: string) => ({
    matches: q.includes("max-width: 899px"), media: q, onchange: null, addEventListener: () => undefined,
    removeEventListener: () => undefined, addListener: () => undefined, removeListener: () => undefined, dispatchEvent: () => false,
  })) as typeof window.matchMedia;
  return () => { window.matchMedia = original; };
}

let restore: () => void = () => undefined;
afterEach(() => { restore(); setPhoneForTests(false); });

describe("bottom tab badges count open picks per tab (B13)", () => {
  it("on a phone, Magic counts its spell picks and Build only the picks on Build tabs", () => {
    const s = snap({ feats: 1, spells: 3 });
    expect(openPicksFor("magic", s, true)).toBe(3);
    expect(openPicksFor("build", s, true)).toBe(1);
    expect(openPicksFor("play", s, true)).toBe(0);
  });

  it("with only Magic Initiate spell picks open, Build shows nothing on a phone", () => {
    const s = snap({ spells: 3 });
    expect(openPicksFor("build", s, true)).toBe(0);
    expect(openPicksFor("magic", s, true)).toBe(3);
  });

  it("a desktop window keeps the single Build count", () => {
    const s = snap({ feats: 1, spells: 3 });
    expect(openPicksFor("build", s, false)).toBe(4);
    expect(openPicksFor("magic", s, false)).toBe(0);
  });

  it("no character, no badge", () => {
    expect(openPicksFor("build", null, true)).toBe(0);
  });

  it("the phone bottom bar badges Magic with its spell picks and Build with the rest", async () => {
    restore = narrowScreen();
    setPhoneForTests(true);
    const api = new OpenPicksApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/characters", { api, openCharacterId: id });
    expect((await screen.findByRole("link", { name: "Magic, 3 choices remaining" })).getAttribute("data-testid")).toBe("nav-magic");
    expect(screen.getByTestId("nav-build").getAttribute("aria-label")).toBe("Build, 1 choices remaining");
  });
});
