import { describe, expect, it } from "vitest";
import { MockApi } from "../mock";

const PACK = { name: "Gloom", code: "GL", edition: "dnd2024" as const };

describe("homebrew creator drafts (WB-06)", () => {
  it("resumes a never-saved element's draft by its draft id", async () => {
    const api = new MockApi();
    const { id: packId } = await api.homebrewCreatePack({ request: PACK });
    const fresh = await api.homebrewNewDraft({ packId, kind: "spell" });
    await api.homebrewUpdateDraft({ draftId: fresh.draftId, form: { ...fresh.form, fields: { name: "Ember Bolt" } } });
    const listed = (await api.homebrewListDrafts()).find((d) => d.elementId === null);
    expect(listed?.draftId).toBe(fresh.draftId);
    const back = await api.homebrewOpenDraft({ packId, draftId: fresh.draftId });
    expect(back.form.fields.name).toBe("Ember Bolt");
    await expect(api.homebrewOpenDraft({ packId })).rejects.toMatchObject({ code: "invalid_argument" });
    await expect(api.homebrewOpenDraft({ packId, draftId: "draft-none" })).rejects.toMatchObject({ code: "not_found" });
  });
});
