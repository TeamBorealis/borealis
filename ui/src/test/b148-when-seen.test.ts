import { afterEach, describe, expect, it, vi } from "vitest";
import { SEEN_SETTLE_MS, whenSeen } from "../lib/whenSeen";

// B-148, seen on the Android emulator: the save resolved as the PDF viewer opened over the app, so the "Saved to" toast
// timed out behind the viewer. The message now waits until the app is back on screen.

let state: DocumentVisibilityState = "visible";
const setVisibility = (v: DocumentVisibilityState) => { state = v; document.dispatchEvent(new Event("visibilitychange")); };

describe("whenSeen", () => {
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); state = "visible"; });

  it("runs after the settle time when the page stays on screen", () => {
    vi.useFakeTimers();
    vi.spyOn(document, "visibilityState", "get").mockImplementation(() => state);
    const fn = vi.fn();
    whenSeen(fn);
    vi.advanceTimersByTime(SEEN_SETTLE_MS - 1);
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("waits while another app covers the page, and runs once on return", () => {
    vi.useFakeTimers();
    vi.spyOn(document, "visibilityState", "get").mockImplementation(() => state);
    const fn = vi.fn();
    whenSeen(fn);
    setVisibility("hidden");
    vi.advanceTimersByTime(SEEN_SETTLE_MS * 10);
    expect(fn).not.toHaveBeenCalled();
    setVisibility("visible");
    expect(fn).toHaveBeenCalledTimes(1);
    setVisibility("hidden");
    setVisibility("visible");
    expect(fn).toHaveBeenCalledTimes(1);
  });
});
