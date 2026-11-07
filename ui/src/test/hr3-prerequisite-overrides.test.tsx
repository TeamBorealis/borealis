import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { CharacterSnapshot } from "../api/contract";
import { MockApi } from "../api/mock";
import { PickerRow } from "../pages/builder/PickerRow";
import { FeaturesCard } from "../pages/play/TraitsPanel";
import type { ShownRow } from "../lib/versions";
import { renderApp, seededId } from "./utils";

/** HR3 (GitHub #24): the prerequisite house rules and the "prerequisites not met" marks. */

const fast = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
const MARK = /prerequisites not met/i;

describe("prerequisite house rules", () => {
  it("are two house rules in the Edition & compatibility flyout, off by default", async () => {
    const user = userEvent.setup();
    const api = fast();
    const id = await seededId(api, "Brannoc");
    renderApp("/builder", { api, openCharacterId: id });
    await user.click(await screen.findByRole("menuitem", { name: /Brannoc/i }));
    await user.click(await screen.findByRole("menuitem", { name: "Edition & compatibility…" }));
    const dialog = await screen.findByRole("dialog", { name: "Edition & compatibility" });
    const feats = within(dialog).getByRole("checkbox", { name: "Ignore feat prerequisites" }) as HTMLInputElement;
    const multi = within(dialog).getByRole("checkbox", { name: "Ignore multiclass prerequisites" }) as HTMLInputElement;
    expect(feats.checked).toBe(false);
    expect(multi.checked).toBe(false);
    await user.click(feats);
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).options.ignoreFeatPrerequisites).toBe(true));
    await user.click(multi);
    await waitFor(async () => expect((await api.getSnapshot({ characterId: id })).options.ignoreMulticlassPrerequisites).toBe(true));
    expect((await api.getSnapshot({ characterId: id })).options.ignoreFeatPrerequisites).toBe(true);
  });

  it("let NEW MULTICLASS take a class whose prerequisite is not met, and mark it", async () => {
    const user = userEvent.setup();
    const api = fast();
    const id = await seededId(api, "Seraphine");
    await api.setCharacterOptions({ characterId: id, options: { ignoreMulticlassPrerequisites: true } });
    renderApp("/sheet", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: "Level Up" }));
    const dlg = await screen.findByRole("dialog", { name: "Level up & multiclassing" });
    const cleric = await within(dlg).findByLabelText("Cleric multiclass");
    const add = within(cleric).getByRole("button", { name: /New multiclass/ }) as HTMLButtonElement;
    expect(add.disabled).toBe(false);
    expect(within(cleric).getByText(MARK)).toBeTruthy();
    const fighter = within(dlg).getByLabelText("Fighter multiclass");
    expect(within(fighter).queryByText(MARK)).toBeNull();
  });
});

describe("prerequisites not met marks", () => {
  const row = (unmet: boolean): ShownRow => {
    const v = { id: "ID_FEAT_GRAPPLER", name: "Grappler", type: "feat", sourceName: "SRD", prerequisitesUnmet: unmet } as ShownRow["shown"];
    return { key: "grappler", name: "Grappler", all: [v], versions: [v], shown: v, chosen: null } as unknown as ShownRow;
  };
  const pickerRow = (unmet: boolean) => (
    <ul>
      <PickerRow row={row(unmet)} listSlot={false} showDescription={false} versionColumn={false} highlighted={false} menuOpen={false}
        notesOpen={false} notesId="n" onToggleNotes={() => {}} onDescribe={() => {}} onChoose={() => {}} onOpenMenu={() => {}} onKeyNav={() => {}} />
    </ul>
  );

  it("marks a picker row offered only by the house rule, and names it for screen readers", () => {
    const { unmount } = render(pickerRow(true));
    expect(screen.getByText(MARK)).toBeTruthy();
    expect(screen.getByRole("button", { name: /Grappler.*prerequisites not met/i })).toBeTruthy();
    unmount();
    render(pickerRow(false));
    expect(screen.queryByText(MARK)).toBeNull();
  });

  it("marks the feat in the play features list", () => {
    const f = { id: "ID_FEAT_GRAPPLER", name: "Grappler", ty: "Feat", source: "SRD", level: 4, description: "", displayed: true };
    render(<FeaturesCard features={[{ ...f, prerequisitesUnmet: true }, { ...f, id: "ID_FEAT_ALERT", name: "Alert" }]} selectedId={null} onSelect={() => {}} />);
    const grappler = screen.getByRole("button", { name: /Grappler/ });
    expect(within(grappler).getByText(MARK)).toBeTruthy();
    expect(within(screen.getByRole("button", { name: /Alert/ })).queryByText(MARK)).toBeNull();
  });

  it("marks the chosen pick's footer and the multiclass in the level panel", async () => {
    /** The mock has no prerequisite engine: mark the chosen Class slot and add a marked multiclass. */
    class MarkedApi extends MockApi {
      private mark(s: CharacterSnapshot): CharacterSnapshot {
        const slots = s.slots.map((x) => (x.elementType === "class" && x.chosen ? { ...x, prerequisitesUnmet: true } : x));
        const extra = { ...s.level.classes[0]!, slotId: "multi-3", classId: "ID_CLASS_FIGHTER", name: "Fighter", isMain: false, level: 1, prerequisitesUnmet: true };
        return { ...s, slots, level: { ...s.level, classes: [...s.level.classes, extra] } };
      }
      override async loadCharacter(a: { characterId: string }) { return this.mark(await super.loadCharacter(a)); }
      override async getSnapshot(a: { characterId: string }) { return this.mark(await super.getSnapshot(a)); }
    }
    const user = userEvent.setup();
    const api = new MarkedApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
    const id = await seededId(api, "Seraphine");
    renderApp("/builder/class", { api, openCharacterId: id });
    const footer = await waitFor(() => {
      const el = document.querySelector(".pick-chosen");
      if (!el) throw new Error("no chosen footer yet");
      return el as HTMLElement;
    });
    expect(within(footer).getByText(MARK)).toBeTruthy();
    await user.click(await screen.findByRole("button", { name: "Level Up" }));
    const dlg = await screen.findByRole("dialog", { name: "Level up & multiclassing" });
    const fighter = within(dlg).getByLabelText("Fighter 1");
    expect(within(fighter).getByText(MARK)).toBeTruthy();
    expect(within(within(dlg).getByLabelText(/^Wizard/)).queryByText(MARK)).toBeNull();
  });
});
