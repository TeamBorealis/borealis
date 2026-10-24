import { describe, expect, it, vi } from "vitest";
import { COMMANDS, EVENTS } from "../contract";
import { createTauriApi } from "../tauri";
import { sanitizeHtml } from "../../components/SafeHtml";

describe("TauriApi", () => {
  it("maps every method to its snake_case command and passes the args object through", async () => {
    const invoke = vi.fn().mockResolvedValue("ok");
    const api = createTauriApi(invoke, vi.fn());
    await api.applySelection({ characterId: "c1", slotId: "s", elementId: "e" });
    expect(invoke).toHaveBeenCalledWith("apply_selection", { characterId: "c1", slotId: "s", elementId: "e" });
    await api.listCharacters();
    expect(invoke).toHaveBeenLastCalledWith("list_characters", undefined);
    expect(api.kind).toBe("tauri");
  });

  it("retries a command that arrives before the app state is managed", async () => {
    const invoke = vi
      .fn()
      .mockRejectedValueOnce("state not managed for field `state` on command `list_characters`")
      .mockResolvedValue([]);
    const api = createTauriApi(invoke, vi.fn());
    await expect(api.listCharacters()).resolves.toEqual([]);
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("does not retry other errors", async () => {
    const invoke = vi.fn().mockRejectedValue({ code: "not_found", message: "nope" });
    const api = createTauriApi(invoke, vi.fn());
    await expect(api.listCharacters()).rejects.toMatchObject({ code: "not_found" });
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("has unique command names and one event name per event", () => {
    const names = Object.values(COMMANDS);
    expect(new Set(names).size).toBe(names.length);
    // local-search-spec.md section 11 names its event `search://index-status`; every other event is `borealis://`.
    expect(Object.values(EVENTS).every((e) => e.startsWith("borealis://") || e === "search://index-status")).toBe(true);
  });

  it("normalises string failures into ApiError", async () => {
    const api = createTauriApi(vi.fn().mockRejectedValue("boom"), vi.fn());
    await expect(api.getSettings()).rejects.toMatchObject({ code: "internal", message: "boom" });
  });

  it("forwards backend events to subscribers and unsubscribes", async () => {
    let cb: (e: { payload: unknown }) => void = () => undefined;
    const unlisten = vi.fn();
    const listen = vi.fn().mockImplementation(async (_name: string, handler: typeof cb) => { cb = handler; return unlisten; });
    const api = createTauriApi(vi.fn(), listen as never);
    const seen: unknown[] = [];
    const off = api.subscribe("content:reloaded", (p) => seen.push(p));
    await Promise.resolve();
    cb({ payload: { reason: "x" } });
    expect(listen).toHaveBeenCalledWith("borealis://content-reloaded", expect.any(Function));
    expect(seen).toEqual([{ reason: "x" }]);
    off();
    expect(unlisten).toHaveBeenCalled();
  });
});

describe("sanitizeHtml", () => {
  it("strips scripts, handlers and unknown tags but keeps formatting", () => {
    const out = sanitizeHtml('<p onclick="x()">Hi <b>there</b><script>alert(1)</script><img src=x onerror=y></p>');
    expect(out).toContain("<b>there</b>");
    expect(out).not.toMatch(/script|onclick|onerror|<img/i);
  });
});
