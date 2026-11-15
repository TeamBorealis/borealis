import { render } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { pinOffset, usePinnedHead } from "../pages/homebrew/class/pinnedHead";

// HC10 Q3 (board item H-aDY): the LEVELS and SPELLCASTING grids keep their header row in view while the form scrolls
// past them, as Aurora's DataGrid does (U7).

describe("pinOffset", () => {
  it("leaves the header where it is while the table's top is in view", () => {
    expect(pinOffset(300, 500, 18, 100)).toBe(0);
  });

  it("moves the header down to the top of the view once the table's top scrolled past it", () => {
    expect(pinOffset(40, 500, 18, 100)).toBe(60);
  });

  it("never moves the header past the table's last row", () => {
    expect(pinOffset(-600, 500, 18, 100)).toBe(482);
  });
});

function Grid() {
  const frame = useRef<HTMLDivElement>(null);
  usePinnedHead(frame);
  return (
    <div className="scroller" style={{ overflowY: "auto" }}>
      <div ref={frame} style={{ overflowX: "auto" }}>
        <table>
          <thead><tr><th scope="col">Level</th><th scope="col">Features</th></tr></thead>
          <tbody><tr><th scope="row">1</th><td /></tr></tbody>
        </table>
      </div>
    </div>
  );
}

const rect = (top: number, height: number): DOMRect => ({ top, height, bottom: top + height, left: 0, right: 600, width: 600, x: 0, y: top, toJSON: () => ({}) });

/** The scroller's top at 100; the table (and its header row) at `tableTop`, 500 high. */
function layout(tableTop: number) {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.classList.contains("scroller")) return rect(100, 400);
    if (this.tagName === "TABLE") return rect(tableTop, 500);
    if (this.tagName === "TH") return rect(tableTop, 18);
    return rect(0, 0);
  });
}

describe("usePinnedHead", () => {
  afterEach(() => vi.restoreAllMocks());

  it("moves every header cell to the top of the scroller when the grid's top scrolled out of view", () => {
    // Arrange
    layout(40);

    // Act
    const { container } = render(<Grid />);

    // Assert: both header cells, not the level numbers' row headers
    const head = Array.from(container.querySelectorAll<HTMLElement>("thead th"));
    expect(head.map((c) => c.style.transform)).toEqual(["translateY(60px)", "translateY(60px)"]);
    expect(head.every((c) => c.classList.contains("hb-pinned"))).toBe(true);
    expect(container.querySelector<HTMLElement>("tbody th")!.style.transform).toBe("");
  });

  it("leaves the header in place while the grid's top is in view", () => {
    // Arrange
    layout(250);

    // Act
    const { container } = render(<Grid />);

    // Assert
    expect(container.querySelector<HTMLElement>("thead th")!.style.transform).toBe("");
  });
});
