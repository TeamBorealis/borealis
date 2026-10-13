import type { KeyboardEvent } from "react";

/** ArrowUp/ArrowDown inside a spell grid row: highlight and focus the neighbouring row (WPF DataGrid behaviour). */
export function arrowNav(e: KeyboardEvent<HTMLElement>, onPick: (id: string) => void): void {
  if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
  const row = (e.target as HTMLElement).closest("tr");
  const sib = e.key === "ArrowDown" ? row?.nextElementSibling : row?.previousElementSibling;
  const id = sib?.getAttribute("data-spell-id");
  if (!sib || !id) return;
  e.preventDefault();
  onPick(id);
  sib.querySelector<HTMLElement>("button")?.focus();
}
