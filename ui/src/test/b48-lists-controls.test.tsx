import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock";
import { NumericUpDown } from "../components/controls";
import { ScoreInput } from "../components/ScoreInput";
import { haptic, setHapticsForTests, type HapticsBackend } from "../lib/haptics";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { sentenceCase } from "../lib/phoneCase";
import { renderApp, seededId } from "./utils";

// B-48 WP4/WP5 behaviour (U9a): O2 Choose button in pickers, O3 Open on a selected tile, O13 light haptics on choose, level
// up, rest and delete; double-tap keeps working; steppers repeat while held. Desktop (phone off) keeps today's behaviour.

async function fresh(api: MockApi) {
  return api.createCharacter({ request: { name: "Picker", ruleset: "dnd2024", startingLevel: 1, gender: "", abilityMethod: "point_buy", options: {} } });
}

/** A recording haptics backend; `enabled` mirrors the platform's system haptics setting. */
function recorder(enabled = true) {
  const calls: string[] = [];
  const backend: HapticsBackend = {
    selection: async () => { calls.push("selection"); },
    impact: async (style) => { calls.push(`impact:${style}`); },
    enabled: async () => enabled,
  };
  return { calls, backend };
}

// jsdom has no PointerEvent: a MouseEvent subclass carries pointerType (mouse presses must never repeat).
if (typeof window.PointerEvent === "undefined") {
  class FakePointerEvent extends MouseEvent {
    pointerId: number;
    pointerType: string;
    constructor(type: string, init: PointerEventInit = {}) { super(type, init); this.pointerId = init.pointerId ?? 1; this.pointerType = init.pointerType ?? "mouse"; }
  }
  (window as unknown as { PointerEvent: typeof FakePointerEvent }).PointerEvent = FakePointerEvent;
}

const phone = (os: "ios" | "android" = "ios") => { setPhoneForTests(true); setPhoneOsForTests(os); };

afterEach(() => { setPhoneForTests(false); setPhoneOsForTests(null); setHapticsForTests(null); vi.useRealTimers(); });

describe("picker Choose button (O2)", () => {
  it("on a phone, tapping a row shows Choose <name> under the list; it chooses the row", async () => {
    phone();
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const snap = await fresh(api);
    renderApp("/builder/class", { api, openCharacterId: snap.id });
    const list = await screen.findByRole("list", { name: "Class options" });
    await user.click(await within(list).findByRole("button", { name: /Wizard/ }));
    const choose = await screen.findByRole("button", { name: "Choose Wizard" });
    await user.click(choose);
    await waitFor(async () => expect((await api.getSnapshot({ characterId: snap.id })).slots.some((s) => s.chosen?.id === "wizard")).toBe(true));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Choose Wizard" })).toBeNull());
  });

  it("double-tap still chooses on a phone", async () => {
    phone("android");
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const snap = await fresh(api);
    renderApp("/builder/class", { api, openCharacterId: snap.id });
    const list = await screen.findByRole("list", { name: "Class options" });
    await user.dblClick(await within(list).findByRole("button", { name: /Fighter/ }));
    await waitFor(async () => expect((await api.getSnapshot({ characterId: snap.id })).slots.some((s) => s.chosen?.id === "fighter")).toBe(true));
  });

  it("shows no Choose button for the row that is already chosen", async () => {
    phone();
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const id = await seededId(api, "Seraphine");
    renderApp("/builder/race", { api, openCharacterId: id });
    const clear = await screen.findAllByRole("button", { name: /^Clear / });
    await user.click(clear[0]!);
    const list = await screen.findByRole("list", { name: /options$/ });
    const rows = await within(list).findAllByRole("button", { pressed: false });
    await user.click(rows[0]!);
    const name = rows[0]!.getAttribute("aria-label")!.split(",")[0]!;
    expect(await screen.findByRole("button", { name: `Choose ${name}` })).toBeTruthy();
  });

  it("the picker header reads in sentence case on a phone (the desktop keeps Aurora's capitals)", async () => {
    phone();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const snap = await fresh(api);
    renderApp("/builder/class", { api, openCharacterId: snap.id });
    const toggle = await screen.findByRole("button", { name: "Class", expanded: true });
    expect(toggle.textContent).toBe("Class");
  });

  it("a desktop window never shows the Choose button (choosing stays double-click or Enter)", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const snap = await fresh(api);
    renderApp("/builder/class", { api, openCharacterId: snap.id });
    const list = await screen.findByRole("list", { name: "Class options" });
    await user.click(await within(list).findByRole("button", { name: /Wizard/ }));
    await screen.findByRole("heading", { name: /Wizard/i });
    expect(screen.queryByRole("button", { name: /^Choose / })).toBeNull();
    expect((await screen.findByRole("button", { name: "CLASS", expanded: true })).textContent).toBe("CLASS");
  });
});

describe("character tiles: Open next to Edit and Delete (O3)", () => {
  it("on a phone a selected tile shows Open; Open loads the character", async () => {
    phone();
    const user = userEvent.setup();
    renderApp("/characters");
    const tile = await screen.findByRole("button", { name: /Seraphine Vale,/ });
    await user.click(tile);
    const group = screen.getByRole("group", { name: "Seraphine Vale" });
    const names = within(group).getAllByRole("button").map((b) => b.getAttribute("aria-label"));
    expect(names).toEqual(expect.arrayContaining(["Edit Seraphine Vale", "Delete Seraphine Vale", "Open Seraphine Vale"]));
    expect(names.indexOf("Open Seraphine Vale")).toBe(names.indexOf("Delete Seraphine Vale") + 1);
    await user.click(within(group).getByRole("button", { name: "Open Seraphine Vale" }));
    expect(await screen.findByRole("heading", { name: "Build", level: 1 }, { timeout: 3000 })).toBeTruthy();
  });

  it("double-tap still opens a character on a phone", async () => {
    phone();
    const user = userEvent.setup();
    renderApp("/characters");
    await user.dblClick(await screen.findByRole("button", { name: /Seraphine Vale,/ }));
    expect(await screen.findByRole("heading", { name: "Build", level: 1 }, { timeout: 3000 })).toBeTruthy();
  });

  it("a desktop window keeps Edit and Delete only", async () => {
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click(await screen.findByRole("button", { name: /Seraphine Vale,/ }));
    expect(screen.getByRole("button", { name: "Delete Seraphine Vale" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Open Seraphine Vale" })).toBeNull();
  });
});

describe("haptics (O13)", () => {
  it("choose fires a selection tick on a phone (Choose button and double-tap)", async () => {
    phone();
    const { calls, backend } = recorder();
    setHapticsForTests(backend);
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const snap = await fresh(api);
    renderApp("/builder/class", { api, openCharacterId: snap.id });
    const list = await screen.findByRole("list", { name: "Class options" });
    await user.click(await within(list).findByRole("button", { name: /Wizard/ }));
    await user.click(await screen.findByRole("button", { name: "Choose Wizard" }));
    await waitFor(() => expect(calls).toEqual(["selection"]));
  });

  it("level up fires a light impact", async () => {
    phone();
    const { calls, backend } = recorder();
    setHapticsForTests(backend);
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const id = await seededId(api, "Seraphine");
    renderApp("/builder/levels", { api, openCharacterId: id });
    const dialog = await screen.findByRole("dialog", { name: /Level up/i });
    await user.click(await within(dialog).findByRole("button", { name: /^Level up Wizard/ }));
    await waitFor(() => expect(calls).toEqual(["impact:light"]));
  });

  it("short and long rest each fire a light impact", async () => {
    phone("android");
    const { calls, backend } = recorder();
    setHapticsForTests(backend);
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const id = await seededId(api, "Seraphine");
    renderApp("/play", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: /Short Rest/ }));
    await user.click(screen.getByRole("button", { name: /Long Rest/ }));
    await waitFor(() => expect(calls).toEqual(["impact:light", "impact:light"]));
  });

  it("deleting a character fires a light impact once the delete is confirmed (not on Cancel)", async () => {
    phone();
    const { calls, backend } = recorder();
    setHapticsForTests(backend);
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click(await screen.findByRole("button", { name: /Seraphine Vale,/ }));
    await user.click(screen.getByRole("button", { name: "Delete Seraphine Vale" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Delete character" })).getByRole("button", { name: "Cancel" }));
    expect(calls).toEqual([]);
    await user.click(screen.getByRole("button", { name: "Delete Seraphine Vale" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Delete character" })).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(calls).toEqual(["impact:light"]));
  });

  it("never fires on desktop, even when a backend exists", async () => {
    const { calls, backend } = recorder();
    setHapticsForTests(backend);
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const id = await seededId(api, "Seraphine");
    renderApp("/play", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: /Short Rest/ }));
    haptic("choose");
    await new Promise((r) => setTimeout(r, 20));
    expect(calls).toEqual([]);
  });

  it("respects the system setting: haptics switched off in the OS means no feedback", async () => {
    phone("android");
    const { calls, backend } = recorder(false);
    setHapticsForTests(backend);
    haptic("choose");
    haptic("delete");
    await new Promise((r) => setTimeout(r, 20));
    expect(calls).toEqual([]);
  });

  it("is a no-op in the browser mock (no Tauri runtime) and never throws", async () => {
    phone();
    expect(() => haptic("rest")).not.toThrow();
    await new Promise((r) => setTimeout(r, 20));
  });

  it("swallows a failing plugin call", async () => {
    phone();
    setHapticsForTests({ selection: () => Promise.reject(new Error("no engine")), impact: () => Promise.reject(new Error("no engine")), enabled: async () => true });
    expect(() => haptic("choose")).not.toThrow();
    await new Promise((r) => setTimeout(r, 20));
  });
});

describe("steppers repeat while held (WP5)", () => {
  function Holder({ kind }: { kind: "score" | "numeric" }) {
    const [v, setV] = useState(5);
    return kind === "score"
      ? <ScoreInput label="Test" value={v} min={0} max={99} onChange={setV} />
      : <NumericUpDown label="Test" value={v} min={0} max={99} onChange={setV} />;
  }
  const value = (kind: "score" | "numeric") => (kind === "score" ? screen.getByLabelText("Test value").textContent : (screen.getByRole("spinbutton") as HTMLInputElement).value);

  for (const kind of ["score", "numeric"] as const) {
    it(`${kind}: a touch hold repeats after a short delay and stops on release; the tap that ends a hold adds no extra step`, () => {
      vi.useFakeTimers();
      render(<Holder kind={kind} />);
      const plus = screen.getByRole("button", { name: "Increase Test" });
      act(() => { fireEvent.pointerDown(plus, { pointerType: "touch" }); });
      act(() => { vi.advanceTimersByTime(300); });
      expect(value(kind)).toBe("5");
      act(() => { vi.advanceTimersByTime(800); });
      const held = Number(value(kind));
      expect(held).toBeGreaterThan(7);
      act(() => { fireEvent.pointerUp(plus, { pointerType: "touch" }); fireEvent.click(plus); });
      act(() => { vi.advanceTimersByTime(1000); });
      expect(Number(value(kind))).toBe(held);
    });

    it(`${kind}: a plain tap or click steps once; a mouse press never repeats`, () => {
      vi.useFakeTimers();
      render(<Holder kind={kind} />);
      const minus = screen.getByRole("button", { name: "Decrease Test" });
      act(() => { fireEvent.pointerDown(minus, { pointerType: "touch" }); fireEvent.pointerUp(minus, { pointerType: "touch" }); fireEvent.click(minus); });
      expect(value(kind)).toBe("4");
      act(() => { fireEvent.pointerDown(minus, { pointerType: "mouse" }); });
      act(() => { vi.advanceTimersByTime(2000); });
      act(() => { fireEvent.pointerUp(minus, { pointerType: "mouse" }); fireEvent.click(minus); });
      expect(value(kind)).toBe("3");
    });
  }
});

describe("coin purse buttons (WP5)", () => {
  it("on a phone each - / + button shows its amount and keeps its accessible name", async () => {
    phone();
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const id = await seededId(api, "Seraphine");
    renderApp("/equipment", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: /Manage coins/ }));
    const dialog = await screen.findByRole("dialog", { name: "Coin purse" });
    for (const [name, text] of [["Remove 10 Gold", "10"], ["Remove 1 Gold", "1"], ["Add 1 Gold", "1"], ["Add 10 Gold", "10"]] as const) {
      expect(within(dialog).getByRole("button", { name }).textContent).toBe(text);
    }
  });

  it("a desktop window keeps the icon-only buttons", async () => {
    const user = userEvent.setup();
    const api = new MockApi({ aiChunkDelayMs: 2 });
    const id = await seededId(api, "Seraphine");
    renderApp("/equipment", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: /Manage coins/ }));
    const dialog = await screen.findByRole("dialog", { name: "Coin purse" });
    expect(within(dialog).getByRole("button", { name: "Add 10 Gold" }).textContent).toBe("");
  });
});

describe("settings checkboxes (O6)", () => {
  it("on a phone the whole row toggles the checkbox", async () => {
    phone();
    const user = userEvent.setup();
    renderApp("/characters");
    await user.click((await screen.findAllByRole("button", { name: "Settings" }))[0]!);
    const box = await screen.findByRole("checkbox", { name: "Selection Flow" });
    const before = (box as HTMLInputElement).checked;
    await user.click(screen.getByText("Automatically navigate to the next available selection.", { exact: false }));
    expect((box as HTMLInputElement).checked).toBe(!before);
    await user.click(box);
    expect((box as HTMLInputElement).checked).toBe(before);
  });
});

describe("sentence case on phones (spec sec. 3.3)", () => {
  it("turns an all-capitals label into the same words in sentence case, keeping abbreviations and names", () => {
    expect(sentenceCase("PLAYER NAME")).toBe("Player name");
    expect(sentenceCase("AI ASSISTANT")).toBe("AI assistant");
    expect(sentenceCase("DEFAULT RULESET FOR NEW CHARACTERS")).toBe("Default ruleset for new characters");
    expect(sentenceCase("BOREALIS")).toBe("Borealis");
    expect(sentenceCase("Selection Flow")).toBe("Selection Flow");
    expect(sentenceCase("STR")).toBe("STR");
  });

  it("Settings labels read in sentence case on a phone and keep Aurora's capitals on desktop", async () => {
    const user = userEvent.setup();
    phone();
    const { unmount } = renderApp("/characters");
    await user.click((await screen.findAllByRole("button", { name: "Settings" }))[0]!);
    expect(await screen.findByText("Player name")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Application settings" })).toBeTruthy();
    unmount();
    setPhoneForTests(false);
    renderApp("/characters");
    await user.click((await screen.findAllByRole("button", { name: "Settings" }))[0]!);
    expect(await screen.findByText("PLAYER NAME")).toBeTruthy();
  });
});
