import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { SplashView } from "../components/splash/SplashView";
import { SplashWindow } from "../components/splash/SplashWindow";
import { StartupGate } from "../components/splash/StartupGate";
import type { StartupBridge, StartupProgress, StartupSignal } from "../lib/startup";

function fakeBridge() {
  let progress: (p: StartupProgress) => void = () => {};
  let done: () => void = () => {};
  const signals: StartupSignal[] = [];
  const quit = vi.fn();
  const bridge: StartupBridge = {
    onProgress: async (h) => { progress = h; return () => {}; },
    onDone: async (h) => { done = h; return () => {}; },
    signal: (s) => { signals.push(s); },
    quit,
  };
  return {
    bridge,
    signals,
    quit,
    emit: (p: Partial<StartupProgress>) => act(() => progress({ message: "x", percent: 0, failed: false, mobile: false, ...p })),
    finish: () => act(() => done()),
  };
}

describe("SplashView", () => {
  it("shows the brand, the status message and a progress bar at the reported percent", () => {
    render(<SplashView progress={{ message: "Loading content packs (2 of 5): srd-5.1", percent: 37, failed: false, mobile: false }} version="1.0.0" />);
    expect(screen.getByText("Borealis")).toBeTruthy();
    expect(screen.getByText("Character builder for fifth edition")).toBeTruthy();
    expect(screen.getByRole("status").textContent).toBe("Loading content packs (2 of 5): srd-5.1");
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("37");
    expect(screen.getByText("37%")).toBeTruthy();
    expect(screen.getByText("1.0.0")).toBeTruthy();
  });

  it("starts at zero before any event arrives and clamps out-of-range percentages", () => {
    const { rerender } = render(<SplashView progress={null} version="1.0.0" />);
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("0");
    rerender(<SplashView progress={{ message: "m", percent: 250, failed: false, mobile: false }} version="1.0.0" />);
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("100");
  });

  it("renders the error state with details and a Close button instead of the progress bar", async () => {
    const onQuit = vi.fn();
    render(<SplashView progress={{ message: "Borealis could not load its data.", percent: 10, failed: true, mobile: false, details: "disk full" }} version="1.0.0" onQuit={onQuit} />);
    expect(screen.getByRole("alert").textContent).toContain("Borealis could not load its data.");
    expect(screen.getByText("Additional details")).toBeTruthy();
    expect(screen.getByText("disk full")).toBeTruthy();
    expect(screen.queryByRole("progressbar")).toBeNull();
    await userEvent.setup().click(screen.getByRole("button", { name: "Close" }));
    expect(onQuit).toHaveBeenCalledOnce();
  });

  it("offers no Close button when the platform cannot close the window", () => {
    render(<SplashView progress={{ message: "failed", percent: 0, failed: true, mobile: true }} version="1.0.0" fullscreen />);
    expect(screen.queryByRole("button", { name: "Close" })).toBeNull();
  });
});

describe("SplashWindow", () => {
  it("follows streamed progress and tells the backend it is listening", async () => {
    const f = fakeBridge();
    render(<SplashWindow bridge={f.bridge} version="1.0.0" />);
    await waitFor(() => expect(f.signals).toEqual(["splash-ready"]));
    await f.emit({ message: "Initializing directories", percent: 4 });
    expect(screen.getByRole("status").textContent).toBe("Initializing directories");
    await f.emit({ message: "Loading settings", percent: 10 });
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("10");
  });

  it("switches to the error state on a failure event and quits through the bridge", async () => {
    const f = fakeBridge();
    render(<SplashWindow bridge={f.bridge} version="1.0.0" />);
    await waitFor(() => expect(f.signals.length).toBe(1));
    await f.emit({ message: "Borealis could not load its data.", percent: 10, failed: true, details: "boom" });
    expect(screen.getByRole("alert")).toBeTruthy();
    await userEvent.setup().click(screen.getByRole("button", { name: "Close" }));
    expect(f.quit).toHaveBeenCalledOnce();
  });
});

describe("StartupGate", () => {
  it("holds the app back until the backend is done, then mounts it and reports ready", async () => {
    const f = fakeBridge();
    const onReady = vi.fn();
    render(<StartupGate bridge={f.bridge} version="1.0.0" onReady={onReady}><div>the app</div></StartupGate>);
    await waitFor(() => expect(f.signals).toEqual(["main-loaded"]));
    expect(screen.queryByText("the app")).toBeNull();
    expect(screen.getByTestId("splash")).toBeTruthy();
    await f.emit({ message: "Starting Borealis", percent: 100 });
    expect(onReady).not.toHaveBeenCalled();
    await f.finish();
    expect(screen.getByText("the app")).toBeTruthy();
    expect(onReady).toHaveBeenCalledOnce();
    await waitFor(() => expect(f.signals).toContain("main-ready"));
    expect(screen.queryByTestId("splash")).toBeNull(); // desktop: no overlay
  });

  it("keeps the overlay up for the minimum time on single-window platforms, then fades it out and removes it", async () => {
    const f = fakeBridge();
    render(<StartupGate bridge={f.bridge} version="1.0.0"><div>the app</div></StartupGate>);
    await waitFor(() => expect(f.signals.length).toBe(1));
    await f.emit({ message: "Starting Borealis", percent: 100, mobile: true });
    await f.finish();
    expect(screen.getByText("the app")).toBeTruthy();
    expect(screen.getByTestId("splash").className).not.toContain("splash--leaving"); // held for the minimum
    await waitFor(() => expect(screen.getByTestId("splash").className).toContain("splash--leaving"), { timeout: 2000 });
    await waitFor(() => expect(screen.queryByTestId("splash")).toBeNull(), { timeout: 1500 });
  });

  it("stays on the splash and never mounts the app when startup fails", async () => {
    const f = fakeBridge();
    render(<StartupGate bridge={f.bridge} version="1.0.0"><div>the app</div></StartupGate>);
    await waitFor(() => expect(f.signals.length).toBe(1));
    await f.emit({ message: "Borealis could not load its data.", percent: 10, failed: true, mobile: true });
    expect(screen.getByRole("alert")).toBeTruthy();
    expect(screen.queryByText("the app")).toBeNull();
  });
});
