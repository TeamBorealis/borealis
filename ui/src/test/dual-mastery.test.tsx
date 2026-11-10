import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Attack, CharacterSnapshot, PlayMastery, PlayState } from "../api/contract";
import { MasteryChooser } from "../pages/equipment/LoadoutTab";
import { AttackTags } from "../pages/manage/AttacksTab";
import { AttackDetail, AttacksPanel } from "../pages/play/AttacksPanel";
import { RollProvider } from "../pages/play/rolls/RollContext";

/**
 * RL10 (RJBPrime, Discord 2026-11-09): a weapon may have more than one mastery property. The host sends each one
 * (`attack.masteries`, `play.masteries[].properties`); every place that showed the one property lists each of them,
 * with its text where the text was shown. A weapon with one property shows exactly what it did.
 */
vi.mock("../state/CharacterContext", () => ({ useCharacter: () => ({ mutate: vi.fn() }) }));

const TOPPLE = "The target must succeed on a Constitution saving throw or have the Prone condition.";
const PUSH = "You can push the creature up to 10 feet straight away from yourself.";

const attack = (over: Partial<Attack>): Attack => ({
  id: "thornglaive", name: "Thornglaive", range: "Melee", attackBonus: "+5", damage: "1d10+3 slashing", description: "", ability: "str",
  auto: true, showInCombat: true, card: false, toHit: 5, critRange: 20, ...over,
});
const dual = attack({ masteries: [{ name: "Topple", text: TOPPLE, mastered: true }, { name: "Push", text: PUSH, mastered: true }] });
const snapshot = (attacks: Attack[]) => ({ attacks, inventory: { items: [] } }) as unknown as CharacterSnapshot;

describe("a weapon with two mastery properties", () => {
  it("Play's attack row tags each mastered property", () => {
    render(<RollProvider characterId="c"><AttacksPanel s={snapshot([dual])} selectedId={null} onSelect={() => {}} /></RollProvider>);
    const row = screen.getByRole("button", { name: "Thornglaive, description" });
    expect(within(row).getByText("Topple")).toBeTruthy();
    expect(within(row).getByText("Push")).toBeTruthy();
  });

  it("Play's attack description gives each property its own heading and text, in order", () => {
    render(<AttackDetail s={snapshot([dual])} attack={dual} />);
    const headings = screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
    expect(headings).toEqual(["Mastery: Topple", "Mastery: Push"]);
    expect(screen.getByText(TOPPLE)).toBeTruthy();
    expect(screen.getByText(PUSH)).toBeTruthy();
  });

  it("Manage's attack tags list each property, unmastered ones marked", () => {
    const unmastered = attack({ masteries: [{ name: "Topple", text: TOPPLE, mastered: false }, { name: "Push", text: PUSH, mastered: false }] });
    render(<AttackTags attack={unmastered} />);
    expect(screen.getByText("Topple (not mastered)").getAttribute("title")).toBe(TOPPLE);
    expect(screen.getByText("Push (not mastered)").getAttribute("title")).toBe(PUSH);
  });

  it("the Weapon Mastery list gives a chosen weapon one line per property; one property stays one line", () => {
    const masteries: PlayMastery[] = [
      { weapon: "thornglaive", name: "Thornglaive", properties: [{ name: "Topple", text: TOPPLE }, { name: "Push", text: PUSH }] },
      { weapon: "club", name: "Club", properties: [{ name: "Slow", text: "Reduce its Speed by 10 feet." }] },
    ];
    const play = { masteries } as unknown as PlayState;
    render(<MasteryChooser capacity={3} play={play} weapons={[{ id: "thornglaive", name: "Thornglaive" }, { id: "club", name: "Club" }]} />);
    const glaive = screen.getByRole("button", { name: "Unmaster Thornglaive" }).closest("li")!;
    expect(within(glaive).getByText(`Topple: ${TOPPLE}`)).toBeTruthy();
    expect(within(glaive).getByText(`Push: ${PUSH}`)).toBeTruthy();
    const club = screen.getByRole("button", { name: "Unmaster Club" }).closest("li")!;
    expect(within(club).getAllByText(/:/).map((e) => e.textContent)).toEqual(["Slow: Reduce its Speed by 10 feet."]);
  });
});
