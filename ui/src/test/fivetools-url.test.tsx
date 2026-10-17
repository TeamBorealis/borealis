import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { UP_TO_DATE } from "../pages/content/indexUpdateText";
import { describeUrlUpdate } from "../pages/content/FiveToolsUrlSection";
import { renderApp } from "./utils";

const REPO = "https://github.com/probe/fivetools-data";
const mock = (extra: ConstructorParameters<typeof MockApi>[0] = {}) => new MockApi({ rng: () => 0.5, ...extra });

async function importFromAddress(api: MockApi, url: string) {
  const user = userEvent.setup();
  renderApp("/content", { api });
  const field = await screen.findByRole("textbox", { name: "5etools data address" });
  await user.type(field, url);
  await user.click(screen.getByRole("button", { name: "Import" }));
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).getByText(/downloaded from github\.com/)).toBeTruthy();
  await user.click(within(dialog).getByRole("button", { name: "Download" }));
  const list = await screen.findByRole("list", { name: "Books sources" });
  await user.click(within(list).getByRole("checkbox", { name: /Basic Rules/ }));
  await user.click(screen.getByRole("button", { name: /Import \d+ book\(s\)/ }));
  expect(await screen.findByRole("heading", { name: "Import report" })).toBeTruthy();
  return user;
}

describe("5etools import from an address", () => {
  for (const mobile of [false, true]) {
    it(`${mobile ? "phones" : "desktop"}: paste, import, then Update says up to date until the address changes`, async () => {
      const api = mock({ mobile });
      const stage = vi.spyOn(api, "stage5etoolsUrl");
      const user = await importFromAddress(api, REPO);
      expect(stage).toHaveBeenCalledWith({ url: REPO });
      const rows = await screen.findByRole("list", { name: "5etools addresses" });
      expect(within(rows).getByText(REPO)).toBeTruthy();
      expect(within(rows).getByText(/4 book\(s\)/)).toBeTruthy();

      await user.click(within(rows).getByRole("button", { name: `Update ${REPO}` }));
      await waitFor(() => expect(screen.getAllByText(UP_TO_DATE).length).toBeGreaterThan(0));

      api.fivetoolsUrls.revision = "ffff00001111";
      await user.click(within(rows).getByRole("button", { name: `Update ${REPO}` }));
      await waitFor(() => expect(screen.getAllByText(/The address changed: \d+ element\(s\) re-imported/).length).toBeGreaterThan(0));
      const [saved] = await api.list5etoolsUrls();
      expect(saved?.revision).toBe("ffff00001111");
      expect(saved?.sources).toHaveLength(4);
    });
  }

  it("shows the refusal of a plain http:// address and imports nothing", async () => {
    const api = mock();
    const user = userEvent.setup();
    renderApp("/content", { api });
    await user.type(await screen.findByRole("textbox", { name: "5etools data address" }), "http://example.org/data");
    await user.click(screen.getByRole("button", { name: "Import" }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Download" }));
    expect((await screen.findByRole("alert")).textContent).toContain("https://");
    expect(screen.queryByRole("list", { name: "Books sources" })).toBeNull();
    expect(await api.list5etoolsUrls()).toEqual([]);
  });

  it("keeps the folder and .zip routes next to the address", async () => {
    renderApp("/content", { api: mock() });
    expect(await screen.findByRole("button", { name: "Scan folder…" })).toBeTruthy();
    expect(screen.getByLabelText("Choose a 5etools .zip")).toBeTruthy();
  });

  it("words the Update outcome", () => {
    expect(describeUrlUpdate(true, null)).toBe(UP_TO_DATE);
  });
});
