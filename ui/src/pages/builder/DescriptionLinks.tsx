import type { ElementLink } from "../../api/contract";
import { SafeHtml } from "../../components";
import { useShell } from "../../components/shell/ShellContext";

/** Up to this many linked cards show open; a longer list (a domain's spells) starts folded, one name per row. */
export const OPEN_LINKS = 3;

/** What a card says about how the element is linked. */
const VIA: Record<ElementLink["via"], string> = {
  grant: "Granted",
  select: "Offered",
  rule: "Used",
  description: "Mentioned",
};

/**
 * Linked Elements (opt-in setting): below the shown element's own text, a card for each element it grants, offers, creates or
 * names (an item an option creates, a spell it grants, a condition it applies, a creature it calls), with that element's text,
 * so a pick can be judged without leaving it. The host decides what is linked from the element data alone; the name opens the
 * element in the compendium.
 */
export function DescriptionLinks({ links }: { links: readonly ElementLink[] }) {
  const { openCompendium } = useShell();
  if (links.length === 0) return null;
  const open = links.length <= OPEN_LINKS;
  return (
    <section className="desc-links" aria-label="Linked elements">
      <h3 className="desc-h">Linked</h3>
      {links.map((l) => (
        <details key={l.id} className="desc-link" open={open}>
          <summary>
            <span className="desc-link-name">{l.name}</span>
            <span className="desc-link-meta">{l.contentType} · {VIA[l.via]}</span>
          </summary>
          <div className="desc-link-body">
            <SafeHtml html={l.descriptionHtml} />
            <p className="desc-line">
              <button type="button" className="desc-source" onClick={() => openCompendium({ elementId: l.id, query: l.name, source: l.source })}>
                Open {l.name} in the compendium
              </button>
            </p>
          </div>
        </details>
      ))}
    </section>
  );
}
