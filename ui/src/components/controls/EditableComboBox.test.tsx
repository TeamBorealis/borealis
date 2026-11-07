import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { EditableComboBox } from ".";

const OPTIONS = ["Blue", "Brown", "Green"];

function Host({ onCommit, initial = "" }: { onCommit: (v: string) => void; initial?: string }) {
  const [v, setV] = useState(initial);
  return (
    <>
      <label htmlFor="eyes">Eyes</label>
      <EditableComboBox id="eyes" label="Eyes" value={v} options={OPTIONS} onChange={setV} onCommit={onCommit} />
      <button type="button">elsewhere</button>
    </>
  );
}

describe("EditableComboBox (issue #30)", () => {
  it("is a collapsed text combobox that keeps typed free text and commits it on blur", async () => {
    const onCommit = vi.fn();
    render(<Host onCommit={onCommit} />);
    const box = screen.getByRole("combobox", { name: "Eyes" });
    expect(box.tagName).toBe("INPUT");
    expect(box.getAttribute("aria-expanded")).toBe("false");
    await userEvent.type(box, "Heterochromia");
    await userEvent.tab();
    expect(onCommit).toHaveBeenLastCalledWith("Heterochromia");
    expect((box as HTMLInputElement).value).toBe("Heterochromia");
  });

  it("opens the list from the arrow button and commits the clicked option", async () => {
    const onCommit = vi.fn();
    render(<Host onCommit={onCommit} />);
    await userEvent.click(screen.getByRole("button", { name: "Show Eyes options" }));
    const list = screen.getByRole("listbox", { name: "Eyes" });
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(OPTIONS);
    expect(list).toBeTruthy();
    await userEvent.click(screen.getByRole("option", { name: "Brown" }));
    expect(onCommit).toHaveBeenLastCalledWith("Brown");
    expect((screen.getByRole("combobox", { name: "Eyes" }) as HTMLInputElement).value).toBe("Brown");
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("walks the list with the arrow keys, picks with Enter and closes with Escape", async () => {
    const onCommit = vi.fn();
    render(<Host onCommit={onCommit} initial="Blue" />);
    const box = screen.getByRole("combobox", { name: "Eyes" });
    await userEvent.click(box);
    await userEvent.keyboard("{ArrowDown}");
    expect(box.getAttribute("aria-expanded")).toBe("true");
    // Opens on the current value.
    const active = () => document.getElementById(box.getAttribute("aria-activedescendant") ?? "")?.textContent;
    expect(active()).toBe("Blue");
    expect(screen.getByRole("option", { name: "Blue" }).getAttribute("aria-selected")).toBe("true");
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(active()).toBe("Green");
    await userEvent.keyboard("{Enter}");
    expect(onCommit).toHaveBeenLastCalledWith("Green");
    expect(box.getAttribute("aria-expanded")).toBe("false");
    await userEvent.keyboard("{ArrowDown}{Escape}");
    expect(box.getAttribute("aria-expanded")).toBe("false");
    expect((box as HTMLInputElement).value).toBe("Green");
  });

  it("commits typed text with Enter while the list is closed", async () => {
    const onCommit = vi.fn();
    render(<Host onCommit={onCommit} />);
    await userEvent.type(screen.getByRole("combobox", { name: "Eyes" }), "Glass Eye{Enter}");
    expect(onCommit).toHaveBeenLastCalledWith("Glass Eye");
  });

  it("closes the open list when focus leaves and commits the text", async () => {
    const onCommit = vi.fn();
    render(<Host onCommit={onCommit} />);
    const box = screen.getByRole("combobox", { name: "Eyes" });
    await userEvent.type(box, "Gold{ArrowDown}");
    expect(screen.getByRole("listbox")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "elsewhere" }));
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(onCommit).toHaveBeenLastCalledWith("Gold");
  });

  it("closes the list when the pointer goes down outside it, even if the text box never had focus", async () => {
    render(<Host onCommit={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Show Eyes options" }));
    expect(screen.getByRole("listbox")).toBeTruthy();
    fireEvent.pointerDown(screen.getByRole("button", { name: "elsewhere" }));
    expect(screen.queryByRole("listbox")).toBeNull();
  });
});
