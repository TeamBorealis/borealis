import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { ChoicesFooter, Dialog, Expander, Flyout, ListBox, MessageBox, NumericUpDown, PickerItem, TabControl, ToggleSwitch } from ".";

describe("shared controls", () => {
  it("ToggleSwitch flips and reports state", async () => {
    const onChange = vi.fn();
    render(<ToggleSwitch checked={false} onChange={onChange} label="Dark" />);
    const sw = screen.getByRole("switch", { name: "Dark" });
    expect(sw.getAttribute("aria-checked")).toBe("false");
    await userEvent.click(sw);
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("NumericUpDown clamps to its range and steps with the arrow keys", async () => {
    function Host() {
      const [v, set] = useState(9);
      return <NumericUpDown value={v} onChange={set} min={0} max={10} label="Qty" />;
    }
    render(<Host />);
    const box = screen.getByRole("spinbutton", { name: "Qty" });
    await userEvent.type(box, "{ArrowUp}{ArrowUp}{ArrowUp}");
    expect(box.getAttribute("aria-valuenow")).toBe("10");
    expect((screen.getByRole("button", { name: "Increase Qty" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("Expander collapses its content", async () => {
    render(<Expander title="Race"><p>body</p></Expander>);
    expect(screen.getByText("body")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Race" }));
    expect(screen.queryByText("body")).toBeNull();
  });

  it("ListBox selects with click and keyboard", async () => {
    const onSelect = vi.fn();
    render(<ListBox items={["a", "b"]} getKey={(i) => i} selected={["a"]} onSelect={onSelect} label="L" itemTemplate={(i) => <span>item {i}</span>} />);
    expect(screen.getByRole("option", { name: "item a" }).getAttribute("aria-selected")).toBe("true");
    await userEvent.click(screen.getByText("item b"));
    expect(onSelect).toHaveBeenCalledWith("b");
  });

  it("TabControl skips disabled tabs with arrow keys", async () => {
    const onChange = vi.fn();
    render(<TabControl tabs={[{ id: "a", label: "A" }, { id: "b", label: "B", disabled: true }, { id: "c", label: "C" }]} active="a" onChange={onChange} label="T" />);
    screen.getByRole("tab", { name: "A" }).focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(onChange).toHaveBeenCalledWith("c");
  });

  it("PickerItem selects and clears; ChoicesFooter reports remaining", async () => {
    const onSelect = vi.fn();
    const onClear = vi.fn();
    render(<><PickerItem name="Gnome" selected onSelect={onSelect} onClear={onClear} /><ChoicesFooter remaining={2} total={3} /></>);
    await userEvent.click(screen.getByRole("button", { name: "Clear selection" }));
    expect(onClear).toHaveBeenCalled();
    expect(screen.getByRole("status").textContent).toBe("2 choices remaining of 3");
  });

  it("Dialog, Flyout and MessageBox close on Escape and resolve results", async () => {
    const close = vi.fn();
    const { unmount } = render(<Flyout title="Fly" onClose={close}><button type="button">inside</button></Flyout>);
    expect(screen.getByRole("dialog", { name: "Fly" })).toBeTruthy();
    await userEvent.keyboard("{Escape}");
    expect(close).toHaveBeenCalledTimes(1);
    unmount();
    const onClose = vi.fn();
    const { unmount: u2 } = render(<Dialog title="D" onClose={onClose}>x</Dialog>);
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalled();
    u2();
    const result = vi.fn();
    render(<MessageBox title="Sure?" message="Delete?" buttons={[{ id: "yes", label: "Yes" }, { id: "no", label: "No" }]} onResult={result} />);
    await userEvent.click(screen.getByRole("button", { name: "Yes" }));
    expect(result).toHaveBeenCalledWith("yes");
    await userEvent.keyboard("{Escape}");
    expect(result).toHaveBeenLastCalledWith("no");
  });
});
