import { fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Dialog } from "../components/controls/Dialog";
import { Flyout } from "../components/controls/Flyout";
import { setPhoneForTests } from "../lib/phone";

// Android WebView, phone sheet: a tap on the title row starts a sheet drag, which captures the pointer; Chrome then
// sends the tap's compatibility mousedown to the scrim, and the scrim closed the sheet (emulator, 2026-10-26). The
// scrim now closes only when the press itself started on the scrim.

// jsdom has no PointerEvent: a MouseEvent carries clientY and pointerId well enough for the sheet drag.
if (typeof window.PointerEvent === "undefined") {
  class FakePointerEvent extends MouseEvent {
    pointerId: number;
    constructor(type: string, init: PointerEventInit = {}) { super(type, init); this.pointerId = init.pointerId ?? 1; }
  }
  (window as unknown as { PointerEvent: typeof FakePointerEvent }).PointerEvent = FakePointerEvent;
}

const overlays = {
  Flyout: (onClose: () => void) => <Flyout title="Compendium" onClose={onClose} position="bottom"><p>body</p></Flyout>,
  Dialog: (onClose: () => void) => <Dialog title="Delete" onClose={onClose}><p>body</p></Dialog>,
};

afterEach(() => setPhoneForTests(false));

describe.each(Object.entries(overlays))("%s scrim", (_name, overlay) => {
  it("a phone tap on the title row does not close it when Chrome sends the mousedown to the scrim", () => {
    setPhoneForTests(true);
    const onClose = vi.fn();
    render(overlay(onClose));
    const title = document.querySelector<HTMLElement>(".flyout-title, .dialog-title, h2")!;
    const scrim = document.querySelector<HTMLElement>(".overlay")!;
    fireEvent.pointerDown(title, { clientY: 80, pointerId: 1 });
    fireEvent.pointerUp(title, { clientY: 80, pointerId: 1 });
    fireEvent.mouseDown(scrim);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("a press that starts on the scrim still closes it", () => {
    const onClose = vi.fn();
    render(overlay(onClose));
    const scrim = document.querySelector<HTMLElement>(".overlay")!;
    fireEvent.pointerDown(scrim);
    fireEvent.mouseDown(scrim);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
