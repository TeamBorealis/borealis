import { useState } from "react";
import type { ElementSummary, Ruleset } from "../../api/contract";
import { Alert, Button, Modal, SearchInput } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useAsync } from "../../state/useAsync";

const LIMIT = 50;

/**
 * Picks a companion creature from the compendium (its `companion` and `monster` elements in the character's edition), or none.
 * Used by Add Companion and by Choose Creature on a companion added by hand.
 */
export function CreaturePicker({ title, ruleset, onPick, onClose }: {
  title: string; ruleset: Ruleset; onPick: (creatureId: string | null) => void; onClose: () => void;
}) {
  const api = useApi();
  const [query, setQuery] = useState("");
  const found = useAsync(
    () => api.searchCompendium({ query: { query, types: ["companion", "monster"], ruleset, limit: LIMIT, offset: 0 } }),
    [api, query, ruleset],
  );
  const rows: ElementSummary[] = found.data?.items ?? [];
  return (
    <Modal title={title} onClose={onClose} footer={<><Button onClick={onClose}>Cancel</Button><Button onClick={() => onPick(null)}>Without a creature</Button></>}>
      <div className="stack cmp-picker">
        <SearchInput label="Search creatures" value={query} onChange={setQuery} data-autofocus />
        {found.error && <Alert tone="danger">{found.error}</Alert>}
        {!found.loading && rows.length === 0 && <p className="mg-sub">No creature matches.</p>}
        <ul className="cmp-picker-list" aria-label="Creatures">
          {rows.map((r) => (
            <li key={r.id}>
              <button type="button" onClick={() => onPick(r.id)}>
                <strong>{r.name}</strong><span>{r.shortDescription || r.sourceName}</span>
              </button>
            </li>
          ))}
        </ul>
        {found.data && found.data.total > rows.length && <p className="mg-sub">Showing {rows.length} of {found.data.total}: type to narrow the list.</p>}
      </div>
    </Modal>
  );
}
