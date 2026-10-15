import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import { renderApp } from "./utils";

const fast = () => new MockApi({ aiChunkDelayMs: 2, importStepMs: 1, aiEnabled: true, rng: () => 0.5 });

async function reviewRunebinder(user: ReturnType<typeof userEvent.setup>) {
  await user.type(await screen.findByLabelText("Paste text"), "Runebinder: a half-caster class that etches runes.");
  await user.click(screen.getByRole("button", { name: "Import" }));
  await screen.findByText("Level 1–20 test passed ✓", {}, { timeout: 3000 });
}

describe("import bug reports (opt-in)", () => {
  it("offers a prominent report when self-checks fail and sends exactly the previewed payload", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/import", { api });
    await reviewRunebinder(user);
    expect(screen.getByText(/self-checks found problems/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Send a report" }));

    const dialog = await screen.findByRole("dialog", { name: "Send a report" });
    expect(within(dialog).getByText(/Nothing is sent unless you press Send/)).toBeTruthy();
    expect(within(dialog).queryByLabelText("Report contents")).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: /Report contents/ }));
    const json = await within(dialog).findByLabelText("Report contents");
    expect(json.textContent).toContain("borealis-import-bug-report/1");
    expect(json.textContent).not.toContain("rewrite the world");

    await user.type(within(dialog).getByLabelText(/What went wrong/), "capstone too strong");
    await user.click(within(dialog).getByLabelText(/Include the source file/));
    expect(within(dialog).getByText(/full text of your source will be sent/)).toBeTruthy();
    await waitFor(() => expect(within(dialog).getByLabelText("Report contents").textContent).toContain("capstone too strong"));
    const send = within(dialog).getByRole("button", { name: "Send" }) as HTMLButtonElement;
    await waitFor(() => expect(send.disabled).toBe(false));
    expect(api.reports.sent).toHaveLength(0);
    const shown: unknown = JSON.parse(within(dialog).getByLabelText("Report contents").textContent ?? "{}");
    await user.click(send);

    expect(await within(dialog).findByText(/Report sent/)).toBeTruthy();
    expect(api.reports.sent).toHaveLength(1);
    const sent = api.reports.sent[0]!;
    expect(sent.note).toBe("capstone too strong");
    expect(sent.source).not.toBeNull();
    expect(shown).toEqual(sent);
    await user.click(within(dialog).getAllByRole("button", { name: "Close" }).at(-1)!);
    expect(screen.queryByRole("dialog", { name: "Send a report" })).toBeNull();
  });

  it("includes recent logs only after the box is ticked and shows them in the preview", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/import", { api });
    await reviewRunebinder(user);
    await user.click(screen.getByRole("button", { name: "Send a report" }));
    const dialog = await screen.findByRole("dialog", { name: "Send a report" });
    const box = await within(dialog).findByLabelText(/Include recent logs \(last 200 lines\)/) as HTMLInputElement;
    expect(box.checked).toBe(false);
    await user.click(within(dialog).getByRole("button", { name: /Report contents/ }));
    const json = await within(dialog).findByLabelText("Report contents");
    expect(json.textContent).not.toContain("Borealis started");

    await user.click(box);
    await waitFor(() => expect(within(dialog).getByLabelText("Report contents").textContent).toContain("Borealis started"));
    const send = within(dialog).getByRole("button", { name: "Send" }) as HTMLButtonElement;
    await waitFor(() => expect(send.disabled).toBe(false));
    const shown: unknown = JSON.parse(within(dialog).getByLabelText("Report contents").textContent ?? "{}");
    await user.click(send);
    await within(dialog).findByText(/Report sent/);
    expect(api.reports.sent[0]).toEqual(shown);
  });

  it("flags one element, shows send failures and sends nothing on cancel", async () => {
    const user = userEvent.setup();
    const api = fast();
    api.reports.failSend = true;
    renderApp("/import", { api });
    await reviewRunebinder(user);
    const row = screen.getByRole("region", { name: "Rune Etching" });
    await user.click(within(row).getByRole("button", { name: /Rune Etching/ }));
    await user.click(within(screen.getByRole("region", { name: "Rune Etching" })).getByRole("button", { name: "Report a problem" }));

    const dialog = await screen.findByRole("dialog", { name: "Send a report" });
    expect(within(dialog).getByText("Marked as wrong: rune-etching")).toBeTruthy();
    const send = within(dialog).getByRole("button", { name: "Send" }) as HTMLButtonElement;
    await waitFor(() => expect(send.disabled).toBe(false));
    await user.click(send);
    expect((await within(dialog).findByRole("alert")).textContent).toMatch(/could not be sent/);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.reports.sent).toHaveLength(0);
  });

  it("keeps the report available but quiet when the checks pass, and after adding to the library", async () => {
    const user = userEvent.setup();
    const api = fast();
    renderApp("/import", { api });
    await user.type(await screen.findByLabelText("Describe it"), "a feat that lets you jump");
    await user.click(screen.getByRole("button", { name: "Feat" }));
    await user.click(screen.getByRole("button", { name: "Import" }));
    await screen.findByRole("region", { name: /Imported feat/ }, { timeout: 3000 });
    expect(screen.queryByText(/self-checks found problems/)).toBeNull();
    expect(screen.getByRole("button", { name: "Send a report" }).className).toContain("btn-ghost");

    await user.click(screen.getByRole("button", { name: "Add to Library" }));
    await screen.findByRole("button", { name: "Open in Compendium" });
    await user.click(screen.getByRole("button", { name: "Send a report" }));
    const dialog = await screen.findByRole("dialog", { name: "Send a report" });
    await user.click(within(dialog).getByRole("button", { name: /Report contents/ }));
    expect((await within(dialog).findByLabelText("Report contents")).textContent).toContain("\"origin\": \"pack\"");
    expect((within(dialog).getByLabelText(/Include the source file/) as HTMLInputElement).disabled).toBe(true);
  });
});
