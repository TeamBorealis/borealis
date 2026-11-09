import type { ElementGrant } from "../../api/contract";
import { SafeHtml } from "../../components";

/** The level a grant with no level gate comes at. */
const FIRST_LEVEL = 1;

const hasText = (g: ElementGrant) => g.descriptionHtml.trim() !== "";

/**
 * The grants of one level. A3: one-line grants (proficiencies, choices) stay a list; a granted trait, feature or feat gets
 * its name as a heading and its text, so the card says what it does.
 */
function GrantBand({ grants, label }: { grants: readonly ElementGrant[]; label: string }) {
  const lines = grants.filter((g) => !hasText(g));
  return (
    <>
      {lines.length > 0 && <ul aria-label={label}>{lines.map((g) => <li key={g.name}>{g.name}</li>)}</ul>}
      {grants.filter(hasText).map((g) => (
        <section key={g.name} className="desc-grant" aria-label={g.name}>
          <h4 className="desc-grant-h">{g.name}</h4>
          <SafeHtml html={g.descriptionHtml} />
        </section>
      ))}
    </>
  );
}

/** The levels the grants come at, in order. */
const levelsOf = (grants: readonly ElementGrant[]): number[] => [...new Set(grants.map((g) => g.level))].sort((a, b) => a - b);

/**
 * The description's Grants list. A6: a class's features up to level 20 read as one wall, so what comes at level 1 shows and
 * the rest folds, level by level, under "Later levels". A list with only level 1 grants (species, backgrounds) stays plain.
 */
export function DescriptionGrants({ grants }: { grants: readonly ElementGrant[] }) {
  if (grants.length === 0) return null;
  const first = grants.filter((g) => g.level <= FIRST_LEVEL);
  const later = grants.filter((g) => g.level > FIRST_LEVEL);
  const split = later.length > 0;
  return (
    <>
      <h3 className="desc-h">Grants</h3>
      {split && first.length > 0 && <p className="desc-line desc-grant-lvl" aria-hidden="true">At level 1</p>}
      <GrantBand grants={first} label={split ? "At level 1" : "Grants"} />
      {split && (
        <details className="desc-later">
          <summary>Later levels</summary>
          {levelsOf(later).map((lv) => (
            <div key={lv} className="desc-grant-level">
              <p className="desc-line desc-grant-lvl" aria-hidden="true">Level {lv}</p>
              <GrantBand grants={later.filter((g) => g.level === lv)} label={`Level ${lv}`} />
            </div>
          ))}
        </details>
      )}
    </>
  );
}
