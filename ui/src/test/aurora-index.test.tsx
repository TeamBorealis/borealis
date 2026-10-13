import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { sourceConsentText } from "../pages/content/IndexesTab";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

const URL = "https://raw.githubusercontent.com/aurorabuilder/elements/master/core.index";

describe("adding an index file asks for consent first", () => {
  it("Cancel adds nothing; Add source adds the index, shown as unverified", async () => {
    const user = userEvent.setup();
    const { api } = renderApp("/content");
    const input = await screen.findByLabelText("Index URL");
    await user.type(input, URL);
    await user.click(screen.getByRole("button", { name: "Fetch" }));
    let dlg = await screen.findByRole("dialog", { name: "Add content source" });
    expect(within(dlg).getByText(sourceConsentText("raw.githubusercontent.com"))).toBeTruthy();
    await user.click(within(dlg).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Add content source" })).toBeNull());
    expect(await api.listIndexes()).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Fetch" }));
    dlg = await screen.findByRole("dialog", { name: "Add content source" });
    await user.click(within(dlg).getByRole("button", { name: "Add source" }));
    const list = await screen.findByRole("list", { name: "Indexes" });
    expect(within(list).getByText("unverified")).toBeTruthy();
    expect(await api.listIndexes()).toHaveLength(1);
  });

  it("a per-index problem is shown in the list", async () => {
    const api = new MockApi({ rng: () => 0.5 });
    const added = (await api.addIndex({ url: URL }))[0]!;
    // The mock backend never fails; give it the reason the real backend reports.
    Object.assign(added, { problem: "the server answered 404 Not Found for 3 files" });
    renderApp("/content", { api });
    expect(await screen.findByText("the server answered 404 Not Found for 3 files")).toBeTruthy();
  });
});
