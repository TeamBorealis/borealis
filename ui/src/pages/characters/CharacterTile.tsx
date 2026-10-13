import type { KeyboardEvent } from "react";
import type { CharacterSummary } from "../../api/contract";
import { Icon } from "../../components/Icon";
import { Portrait } from "../../components";

/** Classic builder build line: "LEVEL 1 MOUNTAIN DWARF FIGHTER". The summary build is "Mountain Dwarf Fighter 1". */
export function buildLine(c: Pick<CharacterSummary, "build" | "level">): string {
  // A character without a class reports "Level n" (or "<Species> Level n") as its build: drop that placeholder so "Level" is not repeated.
  const build = c.build.replace(/\s*\bLevel\s+\d+$/, "").replace(/\s+\d+(?=\s*\/|$)/g, "").trim();
  return build ? `Level ${c.level} ${build}` : `Level ${c.level}`;
}

/**
 * Classic builder collection tile (160 px, 2 px border, 3 px padding): portrait with the name and build line overlaid on a dark band.
 * One click selects (edit and delete corner buttons appear), a double click or Enter loads.
 */
export function CharacterTile({ c, selected, loaded, onSelect, onLoad, onEdit, onDelete }: {
  c: CharacterSummary; selected: boolean; loaded: boolean; onSelect: () => void; onLoad: () => void; onEdit: () => void; onDelete: () => void;
}) {
  const key = (e: KeyboardEvent) => {
    if (e.key === "Enter") { e.preventDefault(); onLoad(); }
  };
  return (
    <div className={`v4-tile ${selected ? "selected" : ""} ${loaded ? "loaded" : ""}`} role="group" aria-label={c.name}>
      {/* aria-current (not aria-pressed): iOS exposes a pressed button as a switch. */}
      <button type="button" className="v4-tile-open" data-testid={`character-tile-${c.id}`} aria-current={selected ? "true" : undefined} aria-label={`${c.name}, ${buildLine(c)}`}
        onClick={onSelect} onDoubleClick={onLoad} onKeyDown={key}>
        <Portrait src={c.portrait} name={c.name} className="v4-tile-img" />
        <span className="v4-tile-text">
          <span className="v4-tile-name">{c.name}</span>
          <span className="v4-tile-build">{buildLine(c)}</span>
        </span>
      </button>
      {selected && (
        <div className="v4-tile-actions">
          <button type="button" className="v4-tile-btn" aria-label={`Edit ${c.name}`} title="Edit" onClick={onEdit}><Icon name="pen" /></button>
          <button type="button" className="v4-tile-btn" aria-label={`Delete ${c.name}`} title="Delete" onClick={onDelete}><Icon name="trash" /></button>
        </div>
      )}
    </div>
  );
}
