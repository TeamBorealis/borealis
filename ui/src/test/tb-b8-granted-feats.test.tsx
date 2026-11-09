import { screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { CharacterSnapshot, PlayFeature, SelectionSlot } from "../api/contract";
import { MockApi } from "../api/mock";
import { setPhoneForTests } from "../lib/phone";
import { grantedFeats } from "../pages/builder/GrantedFeats";
import { parseRules, readCss } from "./cssRules";
import { renderApp, seededId } from "./utils";

// Tester B8 (B-132, TV2): the Feats tab said "no selections" although the background had granted Magic Initiate. On a phone
// it lists granted feats read-only ("Magic Initiate (Cleric), from Acolyte"); since U24 desktop lists them too.
const feat = (over: Partial<PlayFeature>): PlayFeature =>
  ({ id: "ID_T_FEAT_MI", name: "Magic Initiate", ty: "Feat", source: "SRD", level: 1, grantedBy: "Acolyte", description: "<p>Two cantrips and a spell.</p>", displayed: true, ...over });
const miSlot = (over: Partial<SelectionSlot>): SelectionSlot =>
  ({ id: "mi-ability", tab: "feats", title: "Spellcasting Ability", groupLabel: "Magic Initiate (Cleric)", elementType: "list", chosen: { id: "1", name: "Wisdom", type: "list" }, locked: false, required: true, ordinal: 1, of: 1, ...over }) as unknown as SelectionSlot;

/** The character's feats: Magic Initiate from its background, and Tough chosen in a feat slot. */
class GrantedApi extends MockApi {
  override async loadCharacter(args: { characterId: string }): Promise<CharacterSnapshot> {
    return withFeats(await super.loadCharacter(args));
  }
  override async getSnapshot(args: { characterId: string }): Promise<CharacterSnapshot> {
    return withFeats(await super.getSnapshot(args));
  }
}
const withFeats = (s: CharacterSnapshot): CharacterSnapshot => ({
  ...s,
  slots: s.slots.filter((x) => x.tab !== "feats"),
  play: { ...s.play, features: [...s.play.features, feat({}), feat({ id: "ID_T_FEAT_TOUGH", name: "Tough", grantedBy: "Ability Score Improvement" })] },
  addedPicks: [],
});

afterEach(() => setPhoneForTests(false));

describe("Feats tab lists the origin feat granted by the background (B8)", () => {
  it("names the feat with its spell list and where it came from, read-only, without the empty message", async () => {
    setPhoneForTests(true);
    const api = new GrantedApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/builder/feats", { api, openCharacterId: id });
    const list = await screen.findByRole("list", { name: "Granted feats" });
    expect(within(list).getByText("Magic Initiate")).toBeTruthy();
    expect(within(list).getByText("from Acolyte")).toBeTruthy();
    expect(within(list).queryByRole("button", { name: /Remove|Clear/ })).toBeNull();
    expect(screen.queryByText(/doesn't have any selections available here/)).toBeNull();
  });

  it("takes the feat's spell-list label from its own picks, and leaves out feats chosen in a slot", () => {
    const s = {
      slots: [miSlot({}), { id: "asi", tab: "feats", title: "Feat", groupLabel: null, elementType: "feat", chosen: { id: "ID_T_FEAT_TOUGH", name: "Tough", type: "feat" }, locked: false }],
      play: { features: [feat({}), feat({ id: "ID_T_FEAT_TOUGH", name: "Tough", grantedBy: "Ability Score Improvement" }), feat({ id: "ID_T_TRAIT", name: "Darkvision", ty: "Racial Trait" })] },
      addedPicks: [{ elementId: "ID_T_FEAT_ADDED" }],
    } as unknown as CharacterSnapshot;
    expect(grantedFeats(s)).toEqual([{ id: "ID_T_FEAT_MI", label: "Magic Initiate (Cleric)", grantedBy: "Acolyte" }]);
  });

  it("a feat granted twice (two levels of one feature) is listed once", () => {
    const s = { slots: [], play: { features: [feat({}), feat({ level: 4 })] }, addedPicks: [] } as unknown as CharacterSnapshot;
    expect(grantedFeats(s)).toHaveLength(1);
  });

  it("desktop lists them too (U24), as locked chosen-pick lines like the Added feats section's", async () => {
    const api = new GrantedApi();
    const id = await seededId(api, "Seraphine");
    renderApp("/builder/feats", { api, openCharacterId: id });
    const list = await screen.findByRole("list", { name: "Granted feats" });
    expect(document.documentElement.classList.contains("phone")).toBe(false);
    const line = within(list).getByText("Magic Initiate").closest(".pick-chosen")!;
    expect(line.querySelector("svg.icon")).toBeTruthy(); // the lock of a locked pick
    expect(within(line as HTMLElement).getByText("from Acolyte").className).toContain("pick-chosen-src");
    expect(within(list).queryByRole("button", { name: /Remove|Clear/ })).toBeNull();
    expect(list.closest("section")?.className).toBe("added-picks granted-feats");
    expect(screen.queryByText(/doesn't have any selections available here/)).toBeNull();
    // Desktop spacing for the leading section; phones keep theirs (styles/phone/build.css).
    const rule = parseRules(readCss("pages/builder/builder.css")).find((r) => r.selectors.includes("html:not(.phone) .added-picks.granted-feats"));
    expect(rule?.body).toMatch(/margin: 0 0 14px 10px/);
  });
});
