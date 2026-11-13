import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { HomebrewFieldSpec } from "../api/contract";
import { MockApi } from "../api/mock/MockApi";
import { MOCK_ITEM_KINDS } from "../api/mock/homebrew-item-kinds";
import { MOCK_KIND_DESCRIPTORS } from "../api/mock/homebrew-kinds";
import { CreatorEnvValue } from "../pages/homebrew/env";
import { FieldView } from "../pages/homebrew/FieldView";
import { ApiProvider } from "../state/ApiContext";

// Category and tag suggestions from loaded content (board HmGs4): the item forms' Category offers the categories loaded
// items use (picker type "Item Category") after its fixed ones, and the shared Tags field suggests the tags loaded
// elements of the kind's own types carry (picker type "Tag" + the kind's element types).

const PACK = { name: "Ember Isles", code: "EMB", edition: "dnd2024" as const };
const fieldOf = (kinds: readonly { id: string; fields: readonly HomebrewFieldSpec[] }[], kind: string, key: string) =>
  kinds.find((k) => k.id === kind)!.fields.find((f) => f.key === key)!;

async function apiWithSaved(kind: string, fields: Record<string, unknown>): Promise<MockApi> {
  const api = new MockApi({ creatorEnabled: true });
  const pack = await api.homebrewCreatePack({ request: PACK });
  const draft = await api.homebrewNewDraft({ packId: pack.id, kind: kind as never });
  await api.homebrewUpdateDraft({ draftId: draft.draftId, form: { ...draft.form, fields: { ...draft.form.fields, ...fields } } });
  await api.homebrewSaveDraft({ draftId: draft.draftId });
  return api;
}

function renderField(api: MockApi, field: HomebrewFieldSpec, kindId: string, value: unknown) {
  render(
    <ApiProvider api={api}>
      <CreatorEnvValue value={{}}>
        <FieldView field={field} value={value} problems={[]} onChange={vi.fn()} kindId={kindId} />
      </CreatorEnvValue>
    </ApiProvider>,
  );
}

describe("category and tag suggestions (mock)", () => {
  it("the descriptors name the suggestion pickers", () => {
    expect(fieldOf(MOCK_ITEM_KINDS, "item", "category").picker?.types).toEqual(["Item Category"]);
    expect(fieldOf(MOCK_KIND_DESCRIPTORS, "feat", "tags").picker?.types).toEqual(["Tag", "Feat"]);
  });

  it("an item's Category offers its fixed categories, then the ones saved items use", async () => {
    const user = userEvent.setup();
    const api = await apiWithSaved("item", { name: "Lantern of Echoes", category: "Curios" });
    renderField(api, fieldOf(MOCK_ITEM_KINDS, "item", "category"), "item", "");
    await user.click(screen.getByRole("button", { name: "Show Category options" }));
    const list = await screen.findByRole("listbox");
    await waitFor(() => expect(within(list).getByRole("option", { name: "Curios" })).toBeTruthy());
    expect(within(list).getByRole("option", { name: "Adventuring Gear" })).toBeTruthy();
  });

  it("a feat's Tags suggest the tags saved feats carry", async () => {
    const user = userEvent.setup();
    const api = await apiWithSaved("feat", { name: "Gloom Step", tags: ["Shadow Feat"] });
    renderField(api, fieldOf(MOCK_KIND_DESCRIPTORS, "feat", "tags"), "feat", []);
    await user.click(screen.getByRole("button", { name: "Show Tags options" }));
    const list = await screen.findByRole("listbox");
    await waitFor(() => expect(within(list).getByRole("option", { name: "Shadow Feat" })).toBeTruthy());
  });
});
