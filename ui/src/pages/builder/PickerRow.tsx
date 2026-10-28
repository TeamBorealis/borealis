import { type KeyboardEvent, type MouseEvent, useRef } from "react";
import { ConvertedTag, editionMarks, EditionNotes, EditionSuffix } from "../../components";
import type { ShownRow, VersionLike } from "../../lib/versions";
import { useApi } from "../../state/ApiContext";
import { useAsync } from "../../state/useAsync";
import { useCharacter } from "../../state/CharacterContext";
import { VersionButton } from "./VersionMenu";

/** Plain-text source of an element, with its edition mark (footer lines and rows the option list no longer carries). */
export function ElementSource({ id }: { id: string }) {
  const api = useApi();
  const { snapshot } = useCharacter();
  const el = useAsync(() => api.getElement({ elementId: id }), [api, id]);
  const d = el.data;
  if (!d) return null;
  const only = d.rulesets.length === 1 ? d.rulesets[0]! : null;
  const other = only && snapshot && only !== snapshot.ruleset ? only : undefined;
  return <>{d.sourceName}<EditionSuffix marks={editionMarks({ edition: other })} sourceName={d.sourceName} /></>;
}

const levelText = (v: VersionLike) => (v.level === undefined ? "" : v.level === 0 ? "Cantrip" : `Level ${v.level}`);

/** Sec. 1.4: "<name>, <source>[, 2014 rules], <n> versions" (plus the spell level on spell rows). */
export function rowLabel(row: ShownRow): string {
  const v = row.shown;
  const marks = editionMarks(v);
  const n = row.versions.length;
  return [row.name, levelText(v), v.sourceName ?? "", marks.badge ? `${marks.badge} rules` : "", n > 1 ? `${n} versions` : ""].filter(Boolean).join(", ");
}

interface Props {
  row: ShownRow;
  /** List-rule slot: entries are not elements and have no source to look up. */
  listSlot: boolean;
  showDescription: boolean;
  /** The list has a version column (some visible row has versions): every row draws its cell. */
  versionColumn: boolean;
  highlighted: boolean;
  menuOpen: boolean;
  notesOpen: boolean;
  /** Id of the converted-notes element (unique on the page). */
  notesId: string;
  onToggleNotes: () => void;
  onDescribe: () => void;
  onChoose: () => void;
  onOpenMenu: (anchor: HTMLElement) => void;
  onKeyNav: (e: KeyboardEvent<HTMLButtonElement>) => void;
}

/** One picker row (B-55): one name, its shown version's source and edition mark, and the version column. */
export function PickerRow({ row, listSlot, showDescription, versionColumn, highlighted, menuOpen, notesOpen, notesId, onToggleNotes, onDescribe, onChoose, onOpenMenu, onKeyNav }: Props) {
  const verRef = useRef<HTMLButtonElement | null>(null);
  const v = row.shown;
  const sel = row.chosen !== null;
  const disabled = !!v.disabledReason;
  const marks = editionMarks(v);
  const multi = row.versions.length > 1;
  const onSource = (e: MouseEvent) => multi && (e.target as Element).closest(".pick-src-name") !== null;
  const level = levelText(v);
  return (
    <li className={`pick-row ${sel ? "selected" : ""} ${highlighted ? "hl" : ""} ${marks.converted ? "is-converted" : ""}`} data-row={row.name}>
      <button type="button" className="pick-main" aria-pressed={sel} aria-disabled={disabled || undefined} title={v.disabledReason}
        aria-label={rowLabel(row)}
        onClick={(e) => { if (onSource(e)) { onOpenMenu(verRef.current ?? e.currentTarget); return; } onDescribe(); }}
        onKeyDown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); if (!disabled && !sel) onChoose(); return; }
          onKeyNav(e);
        }}
        onDoubleClick={(e) => { if (!onSource(e) && !sel && !disabled) onChoose(); }}>
        <span className={`pick-name ${disabled ? "disabled" : ""}`} aria-hidden="true">
          {row.name}{level && <span className="pick-sub"> {level}</span>}
          {v.prerequisites && <span className="pick-sub"> - Requires {v.prerequisites}</span>}
        </span>
        {showDescription && <span className="pick-desc" aria-hidden="true">{v.shortDescription}</span>}
        <span className="pick-src" aria-hidden="true">
          <span className={`pick-src-name ${multi ? "has-versions" : ""}`} title={multi ? `${row.versions.length} versions: choose a source` : undefined}>
            {v.sourceName !== undefined
              ? <>{v.sourceName}<EditionSuffix marks={marks} sourceName={v.sourceName} /></>
              : (listSlot ? null : <ElementSource id={v.id} />)}
          </span>
        </span>
      </button>
      {versionColumn && (
        <span className="pick-ver">
          {multi && <VersionButton count={row.versions.length} rowName={row.name} open={menuOpen} onOpen={onOpenMenu} ref={verRef} />}
        </span>
      )}
      <ConvertedTag marks={marks} open={notesOpen} notesId={notesId} onToggle={onToggleNotes} />
      <EditionNotes marks={marks} id={notesId} open={notesOpen} className="pick-notes" />
    </li>
  );
}
