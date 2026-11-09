import { useState, type ReactNode } from "react";

/**
 * The character's content notices as ONE compact notice (GitHub #11, C1): a single notice is shown as it is; several are a
 * count that expands into the list, so a Build page never opens with a wall of banners. Content problems the character
 * meets on a choice are shown on that choice instead (`SelectionSlot.problem`); the full list stays in the import report.
 * `action` (GitHub #37: Show missing content) stays visible whether the list is open or not.
 */
export function NoticesBanner({ notices, action }: { notices: string[]; action?: ReactNode }) {
  const [open, setOpen] = useState(false);
  if (notices.length === 0) return null;
  if (notices.length === 1) {
    return <section className="alert info bp-notices" aria-label="Content notices">{notices[0]}{action}</section>;
  }
  return (
    <section className="alert info bp-notices" aria-label="Content notices">
      <button type="button" className="bp-notices-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
        {notices.length} content notices<span aria-hidden="true">{open ? " ▾" : " ▸"}</span>
      </button>
      {action}
      {open && <ul className="bp-notices-list">{notices.map((n) => <li key={n}>{n}</li>)}</ul>}
    </section>
  );
}
