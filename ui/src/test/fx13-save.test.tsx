import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { useCharacter } from "../state/CharacterContext";
import { getStatus } from "../state/status";
import { sanitizeFileName } from "../components/shell/SaveFlyout";
import { renderApp, seededId } from "./utils";
import { AppProviders } from "../AppProviders";

const fast = () => new MockApi({ aiChunkDelayMs: 1, rng: () => 0.5 });

describe("Save flyout", () => {
  it("Ctrl+S opens the flyout, Save character reports the name in the status bar, with no toast", async () => {
    const user = userEvent.setup();
    const api = fast();
    const id = await seededId(api, "Brannoc");
    renderApp("/builder", { api, openCharacterId: id });
    await screen.findByRole("menuitem", { name: /Brannoc/i });
    await user.keyboard("{Control>}s{/Control}");
    const dialog = await screen.findByRole("dialog", { name: "Save character" });
    expect((within(dialog).getByLabelText("Filename") as HTMLInputElement).value).toBe("Brannoc Ironvale");
    expect(within(dialog).getByRole("checkbox", { name: /favorite/i })).toBeTruthy();
    expect(within(dialog).getByLabelText("Group")).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Save character" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Save character" })).toBeNull());
    expect(getStatus()).toBe("'Brannoc Ironvale' Saved");
    expect(screen.queryByText("Saved", { selector: ".toast *, .toast" })).toBeNull();
  });

  it("Cancel closes without saving", async () => {
    const user = userEvent.setup();
    const api = fast();
    const id = await seededId(api, "Brannoc");
    renderApp("/builder", { api, openCharacterId: id });
    await screen.findByRole("menuitem", { name: /Brannoc/i });
    await user.keyboard("{Control>}s{/Control}");
    const dialog = await screen.findByRole("dialog", { name: "Save character" });
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog", { name: "Save character" })).toBeNull();
    expect(getStatus()).not.toMatch(/Saved$/);
  });

  it("strips illegal filename characters", () => {
    expect(sanitizeFileName('a/b:c*d?"e<f>g|h')).toBe("abcdefgh");
  });
});

function Probe({ api, id }: { api: MockApi; id: string }) {
  const c = useCharacter();
  return (
    <div>
      <span data-testid="flag">{String(c.savedByUser)}</span>
      <button onClick={() => void api.getSnapshot({ characterId: id }).then(c.adopt)}>adopt</button>
      <button onClick={() => void c.commitSave({ name: "Brannoc Ironvale", group: "Characters", favorite: false })}>commit</button>
    </div>
  );
}

describe("savedByUser flag", () => {
  it("stays false for a new character and for autosave, true after a user save", async () => {
    const api = fast();
    const id = await seededId(api, "Brannoc");
    render(<AppProviders api={api}><Probe api={api} id={id} /></AppProviders>);
    const user = userEvent.setup();
    await user.click(screen.getByText("adopt"));
    await waitFor(() => expect(screen.getByTestId("flag").textContent).toBe("false"));
    await act(async () => { await new Promise((r) => setTimeout(r, 1800)); });
    expect(screen.getByTestId("flag").textContent).toBe("false");
    await user.click(screen.getByText("commit"));
    await waitFor(() => expect(screen.getByTestId("flag").textContent).toBe("true"));
  });
});
