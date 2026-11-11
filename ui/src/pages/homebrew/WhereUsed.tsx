import { useEffect, useState } from "react";
import type { HomebrewElementRef, HomebrewUseRelation } from "../../api/contract";
import { useShell } from "../../components";
import { useApi } from "../../state/ApiContext";
import { errorMessage } from "../../state/useAsync";
import { Band } from "../spells/TwoPane";

/** What the using element does with this one, in plain words. */
export const RELATION_WORDS: Readonly<Record<HomebrewUseRelation, string>> = {
  grants: "Gives it",
  selects: "Offers it as a choice",
  requires: "Needs it",
  lists: "Lists it",
};

export interface WhereUsedProps {
  /** The saved element (an unsaved one is used by nothing yet). */
  elementId: string | null;
  /** Fetch again when this changes (after a save). */
  version?: unknown;
  /** The user's homebrew packs: their elements open in the creator, every other one in the Compendium. */
  homebrewPackIds: ReadonlySet<string>;
  onOpen: (ref: HomebrewElementRef) => void;
  /** Edit a loaded (bundled or imported) element: the creator makes a copy of it in this pack and opens it (HC5). */
  onEdit?: (ref: HomebrewElementRef) => void;
}

/**
 * WHERE USED (spec 12.3, the DETAILS panel under the live description): the loaded elements, own and other, that give,
 * offer, need or list this one. Each opens: a homebrew element in the creator, any other in the Compendium. A loaded
 * (bundled or imported) one also offers Edit, which edits a copy of it in this pack (HC5: imported packs are copy-only).
 */
export function WhereUsed({ elementId, version, homebrewPackIds, onOpen, onEdit }: WhereUsedProps) {
  const api = useApi();
  const { openCompendium } = useShell();
  const [refs, setRefs] = useState<HomebrewElementRef[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!elementId) { setRefs(null); return undefined; }
    let live = true;
    setError(null);
    api.homebrewWhereUsed({ elementId })
      .then((r) => { if (live) setRefs(r); })
      .catch((e) => { if (live) { setRefs([]); setError(errorMessage(e)); } });
    return () => { live = false; };
  }, [api, elementId, version]);
  if (!elementId) return null;
  const open = (r: HomebrewElementRef) => {
    if (r.packId && homebrewPackIds.has(r.packId)) onOpen(r);
    else openCompendium({ elementId: r.id });
  };
  const loaded = (r: HomebrewElementRef) => !(r.packId && homebrewPackIds.has(r.packId));
  return (
    <section className="hb-where" aria-label="Where used">
      <Band title={`Where used${refs ? ` (${refs.length})` : ""}`} />
      {error && <p className="hb-error-line" role="alert">{error}</p>}
      {refs === null ? <p className="hb-pending">Looking...</p>
        : refs.length === 0 ? <p className="hb-pending">Nothing uses it yet.</p>
          : (
            <ul className="hb-where-list">
              {refs.map((r) => (
                <li key={`${r.relation}:${r.id}`}>
                  <button type="button" className="ap-name" onClick={() => open(r)}>{r.name}</button>
                  <span className="hb-where-what">{RELATION_WORDS[r.relation]} · {r.type}</span>
                  {onEdit && loaded(r) && (
                    <button type="button" className="hb-tool hb-where-edit" aria-label={`Edit ${r.name}`}
                      title="Edit a copy of it in this pack. The original stays as it is." onClick={() => onEdit(r)}>Edit</button>
                  )}
                </li>
              ))}
            </ul>
          )}
    </section>
  );
}
