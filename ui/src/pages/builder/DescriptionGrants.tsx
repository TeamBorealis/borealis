import type { ElementGrant } from "../../api/contract";

/** The level a grant with no level gate comes at. */
const FIRST_LEVEL = 1;

/**
 * The description's Grants list. A6: a class's features up to level 20 read as one wall, so what comes at level 1 shows and
 * the rest folds, in level order, under "Later levels". A list with only level 1 grants (species, backgrounds) stays plain.
 */
export function DescriptionGrants({ grants }: { grants: readonly ElementGrant[] }) {
  if (grants.length === 0) return null;
  const first = grants.filter((g) => g.level <= FIRST_LEVEL);
  const later = grants.filter((g) => g.level > FIRST_LEVEL);
  return (
    <>
      <h3 className="desc-h">Grants</h3>
      {later.length > 0 && first.length > 0 && <p className="desc-line desc-grant-lvl" aria-hidden="true">At level 1</p>}
      {first.length > 0 && (
        <ul aria-label={later.length > 0 ? "At level 1" : "Grants"}>
          {first.map((g) => <li key={g.name}>{g.name}</li>)}
        </ul>
      )}
      {later.length > 0 && (
        <details className="desc-later">
          <summary>Later levels</summary>
          <ul>
            {later.map((g) => <li key={`${g.level}:${g.name}`}><span className="desc-grant-lvl">Level {g.level}</span> {g.name}</li>)}
          </ul>
        </details>
      )}
    </>
  );
}
