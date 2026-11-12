import { type RefObject, useLayoutEffect } from "react";

/**
 * How far the header row moves down to stay in view (HC10 Q3, U7: Aurora's DataGrid keeps its header): the distance
 * from the table's top to the top of the visible area, never above the table and never past its last row.
 *
 * @param tableTop    the table's top on screen
 * @param tableHeight the table's height
 * @param headHeight  the header row's height
 * @param viewTop     the top of the visible area (the scroller's top, or the bottom of a sticky head over it)
 */
export function pinOffset(tableTop: number, tableHeight: number, headHeight: number, viewTop: number): number {
  const room = Math.max(0, tableHeight - headHeight);
  return Math.min(Math.max(0, viewTop - tableTop), room);
}

/** The nearest ancestor that scrolls up and down. */
function scrollParent(el: HTMLElement): HTMLElement | null {
  for (let p = el.parentElement; p; p = p.parentElement) {
    if (/(auto|scroll)/.test(getComputedStyle(p).overflowY)) return p;
  }
  return null;
}

/** The top of the visible area: the scroller's top, or lower when the editor's sticky head (phones) covers it. */
function viewTopOf(scroller: HTMLElement, table: HTMLElement): number {
  const top = scroller.getBoundingClientRect().top + scroller.clientTop;
  const head = table.closest(".hb-editor")?.querySelector<HTMLElement>(".hb-editor-head");
  if (!head || getComputedStyle(head).position !== "sticky") return top;
  return Math.max(top, head.getBoundingClientRect().bottom);
}

/**
 * Pins a grid's header row while the grid is on screen (the LEVELS and SPELLCASTING grids, HC10 Q3). The grid's frame
 * scrolls sideways, which captures `position: sticky`, so the header cells are moved down by the scroll instead.
 */
export function usePinnedHead(frame: RefObject<HTMLElement>, deps: readonly unknown[] = []): void {
  useLayoutEffect(() => {
    const box = frame.current;
    const table = box?.querySelector("table");
    const cells = table ? Array.from(table.querySelectorAll<HTMLElement>("thead th")) : [];
    if (!box || !table || cells.length === 0) return undefined;
    // above the frame: its own sideways scroll computes as overflow-y auto too
    const scroller = scrollParent(box);
    if (!scroller) return undefined;
    let raf = 0;
    let shift = 0;
    const place = () => {
      raf = 0;
      const rect = table.getBoundingClientRect();
      const first = cells[0]!;
      // the header cells start a border below the table's edge (measured without the shift this hook applied)
      const inset = first.getBoundingClientRect().top - shift - rect.top;
      const y = Math.round(pinOffset(rect.top + inset, rect.height - inset, first.offsetHeight, viewTopOf(scroller, table)));
      shift = y;
      for (const c of cells) {
        c.style.transform = y > 0 ? `translateY(${y}px)` : "";
        c.classList.toggle("hb-pinned", y > 0);
      }
    };
    const later = () => { if (!raf) raf = requestAnimationFrame(place); };
    place();
    scroller.addEventListener("scroll", later, { passive: true });
    window.addEventListener("resize", later);
    const ro = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(later);
    ro?.observe(table);
    return () => {
      if (raf) cancelAnimationFrame(raf);
      scroller.removeEventListener("scroll", later);
      window.removeEventListener("resize", later);
      ro?.disconnect();
      for (const c of cells) { c.style.transform = ""; c.classList.remove("hb-pinned"); }
    };
  }, deps); // eslint-disable-line react-hooks/exhaustive-deps
}
