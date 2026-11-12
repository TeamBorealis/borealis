import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setPhoneForTests } from "../../lib/phone";
import { SelectField } from "../Fields";
import { ComboBox, type ComboOption } from "./Controls";

// U22 (B-138, tester C2): on phones every select opens the app's bottom sheet (Dialog: grabber, swipe down cancels) with
// one 48 dp row per option, the current one marked; the WebView's select dialog picked the wrong row when a finger
// drifted. Desktop keeps the native select (U7).

const CATEGORIES: ComboOption[] = [
  { value: "Adventuring Gear", label: "Adventuring Gear (40)" },
  { value: "Equipment Packs", label: "Equipment Packs (7)" },
  { value: "Tools", label: "Tools (12)" },
  { value: "Trade Goods", label: "Trade Goods (23)" },
];

afterEach(() => { cleanup(); setPhoneForTests(false); });

// jsdom has no PointerEvent: a MouseEvent carries clientY and pointerId well enough for the sheet drag (as b48-sheets).
if (typeof window.PointerEvent === "undefined") {
  class FakePointerEvent extends MouseEvent {
    pointerId: number;
    constructor(type: string, init: PointerEventInit = {}) { super(type, init); this.pointerId = init.pointerId ?? 1; }
  }
  (window as unknown as { PointerEvent: typeof FakePointerEvent }).PointerEvent = FakePointerEvent;
}

function Category({ onChange, options = CATEGORIES, start = "Adventuring Gear" }: { onChange?: (v: string) => void; options?: ComboOption[]; start?: string }) {
  const [value, setValue] = useState(start);
  return <ComboBox aria-label="Category" value={value} options={options} onChange={(v) => { setValue(v); onChange?.(v); }} />;
}

const trigger = () => screen.getByRole("button", { name: "Category" });
const sheet = () => screen.getByRole("dialog", { name: "Category" });
const rows = () => within(sheet()).getAllByRole("option");

describe("ComboBox on desktop", () => {
  it("renders the native select and reports the chosen value", async () => {
    // Arrange
    const onChange = vi.fn();
    render(<Category onChange={onChange} />);
    const select = screen.getByRole("combobox", { name: "Category" }) as HTMLSelectElement;

    // Act
    await userEvent.selectOptions(select, "Equipment Packs");

    // Assert
    expect(select.tagName).toBe("SELECT");
    expect(onChange).toHaveBeenCalledWith("Equipment Packs");
    expect(screen.queryByRole("button", { name: "Category" })).toBeNull();
  });
});

describe("ComboBox on a phone", () => {
  it("renders a button with the current label instead of a select", () => {
    // Arrange
    setPhoneForTests(true);

    // Act
    render(<Category />);

    // Assert
    expect(document.querySelector("select")).toBeNull();
    const button = trigger();
    expect(button.textContent).toBe("Adventuring Gear (40)");
    // No aria-haspopup: iOS WebKit then drops the button trait, and VoiceOver and XCTest see "other", not a button
    // (the native select it replaces is a button; mobile e2e m9). aria-expanded still says whether the sheet is open.
    expect(button.hasAttribute("aria-haspopup")).toBe(false);
    expect(button.getAttribute("aria-expanded")).toBe("false");
    expect(screen.getByRole("button", { name: "Category", description: "Adventuring Gear (40)" })).toBe(button);
  });

  it("opens a labelled bottom sheet with one row per option and the current one marked and focused", async () => {
    // Arrange
    setPhoneForTests(true);
    render(<Category start="Tools" />);

    // Act
    await userEvent.click(trigger());

    // Assert
    const dialog = sheet();
    expect(dialog.querySelector(".sheet-grabber")).toBeTruthy();
    expect(trigger().getAttribute("aria-expanded")).toBe("true");
    expect(within(dialog).getByRole("listbox", { name: "Category" })).toBeTruthy();
    expect(rows().map((r) => r.textContent)).toEqual(CATEGORIES.map((c) => c.label));
    const current = rows().filter((r) => r.getAttribute("aria-selected") === "true");
    expect(current.map((r) => r.textContent)).toEqual(["Tools (12)"]);
    expect(current[0]!.querySelector(".icon")).toBeTruthy();
    expect(document.activeElement).toBe(current[0]);
  });

  it("choosing a row fires onChange with that row's value, closes the sheet and returns focus", async () => {
    // Arrange
    setPhoneForTests(true);
    const onChange = vi.fn();
    render(<Category onChange={onChange} />);
    await userEvent.click(trigger());

    // Act
    await userEvent.click(within(sheet()).getByRole("option", { name: "Equipment Packs (7)" }));

    // Assert
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith("Equipment Packs");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(trigger().textContent).toBe("Equipment Packs (7)");
    expect(document.activeElement).toBe(trigger());
  });

  it("choosing the current row again closes without a change", async () => {
    // Arrange
    setPhoneForTests(true);
    const onChange = vi.fn();
    render(<Category onChange={onChange} />);
    await userEvent.click(trigger());

    // Act
    await userEvent.click(within(sheet()).getByRole("option", { name: "Adventuring Gear (40)" }));

    // Assert
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("a swipe down on the grabber cancels without a change", async () => {
    // Arrange
    setPhoneForTests(true);
    const onChange = vi.fn();
    render(<Category onChange={onChange} />);
    await userEvent.click(trigger());
    const grabber = sheet().querySelector<HTMLElement>(".sheet-grabber")!;

    // Act
    fireEvent.pointerDown(grabber, { clientY: 500, pointerId: 1 });
    fireEvent.pointerMove(grabber, { clientY: 640, pointerId: 1 });
    fireEvent.pointerUp(grabber, { clientY: 640, pointerId: 1 });

    // Assert
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(onChange).not.toHaveBeenCalled();
    expect(trigger().textContent).toBe("Adventuring Gear (40)");
    expect(document.activeElement).toBe(trigger());
  });

  it("Close and Escape cancel without a change", async () => {
    // Arrange
    setPhoneForTests(true);
    const onChange = vi.fn();
    render(<Category onChange={onChange} />);

    // Act
    await userEvent.click(trigger());
    await userEvent.click(within(sheet()).getByRole("button", { name: "Close" }));
    await userEvent.click(trigger());
    await userEvent.keyboard("{Escape}");

    // Assert
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(onChange).not.toHaveBeenCalled();
  });

  it("the keyboard opens the sheet, moves between rows and chooses with Enter", async () => {
    // Arrange
    setPhoneForTests(true);
    const onChange = vi.fn();
    render(<Category onChange={onChange} />);
    trigger().focus();

    // Act
    await userEvent.keyboard("{ArrowDown}");
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{ArrowUp}{Enter}");

    // Assert
    expect(onChange).toHaveBeenCalledWith("Equipment Packs");
  });

  it("disabled options are shown but cannot be chosen", async () => {
    // Arrange
    setPhoneForTests(true);
    const onChange = vi.fn();
    const options = CATEGORIES.map((c) => (c.value === "Tools" ? { ...c, disabled: true } : c));
    render(<Category onChange={onChange} options={options} />);
    await userEvent.click(trigger());

    // Act
    const tools = within(sheet()).getByRole("option", { name: "Tools (12)" });
    await userEvent.click(tools);

    // Assert
    expect((tools as HTMLButtonElement).disabled).toBe(true);
    expect(onChange).not.toHaveBeenCalled();
    expect(sheet()).toBeTruthy();
  });

  it("a disabled select does not open", async () => {
    // Arrange
    setPhoneForTests(true);
    render(<ComboBox aria-label="Category" value="Tools" options={CATEGORIES} onChange={vi.fn()} disabled />);

    // Act
    await userEvent.click(trigger());

    // Assert
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("a tap on a row does not reach a clickable ancestor of the select", async () => {
    // Arrange
    setPhoneForTests(true);
    const outer = vi.fn();
    render(<div onClick={outer}><ComboBox aria-label="Category" value="Tools" options={CATEGORIES} onChange={vi.fn()} /></div>);
    await userEvent.click(trigger());
    outer.mockClear();

    // Act
    await userEvent.click(within(sheet()).getByRole("option", { name: "Trade Goods (23)" }));

    // Assert
    expect(outer).not.toHaveBeenCalled();
  });
});

describe("SelectField", () => {
  const RESETS: ComboOption[] = [{ value: "short", label: "Short Rest" }, { value: "long", label: "Long Rest" }];

  it("is a native select on desktop", async () => {
    // Arrange
    const onChange = vi.fn();
    render(<SelectField label="Resets on" value="short" options={RESETS} onChange={onChange} />);

    // Act
    await userEvent.selectOptions(screen.getByLabelText("Resets on"), "long");

    // Assert
    expect(screen.getByLabelText("Resets on").tagName).toBe("SELECT");
    expect(onChange).toHaveBeenCalledWith("long");
  });

  it("on a phone its label names the button and titles the sheet", async () => {
    // Arrange
    setPhoneForTests(true);
    const onChange = vi.fn();
    render(<SelectField label="Resets on" value="short" options={RESETS} onChange={onChange} />);

    // Act
    await userEvent.click(screen.getByRole("button", { name: "Resets on" }));
    await userEvent.click(within(screen.getByRole("dialog", { name: "Resets on" })).getByRole("option", { name: "Long Rest" }));

    // Assert
    expect(document.querySelector("select")).toBeNull();
    expect(onChange).toHaveBeenCalledWith("long");
  });
});
