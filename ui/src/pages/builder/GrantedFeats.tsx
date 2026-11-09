import type { CharacterSnapshot } from "../../api/contract";
import { Icon } from "../../components";

/** A feat the build grants outright (a background's origin feat), not one picked in a slot or added by hand. */
export interface GrantedFeat {
  id: string;
  /** The feat's name, with its spell list when its picks carry one ("Magic Initiate (Cleric)"). */
  label: string;
  /** The element that granted it ("Acolyte"). */
  grantedBy: string | null;
}

/**
 * Tester B8 (B-132, TV2): the feats the build grants without a pick. A feat chosen in an open slot is already on the tab
 * as that slot's chosen line, and one added by hand under Added feats; a granted feat's picks (Magic Initiate's spell list)
 * carry the feat's full name as their group label, which names it here.
 */
export function grantedFeats(s: CharacterSnapshot): GrantedFeat[] {
  const picked = new Set(s.slots.filter((x) => !x.locked && x.chosen).map((x) => x.chosen!.id));
  const out: GrantedFeat[] = [];
  for (const f of s.play.features) {
    if (f.ty !== "Feat" || f.added || picked.has(f.id) || out.some((g) => g.id === f.id)) continue;
    const label = s.slots.find((x) => x.groupLabel?.startsWith(`${f.name} (`))?.groupLabel ?? f.name;
    out.push({ id: f.id, label, grantedBy: f.grantedBy ?? null });
  }
  return out;
}

interface GrantedFeatsProps {
  feats: GrantedFeat[];
  /** Shows a feat in the page's description pane. */
  onDescribe: (id: string) => void;
}

/** Phones (B8): the granted feats as read-only chosen lines, ahead of the tab's own picks. */
export function GrantedFeats({ feats, onDescribe }: GrantedFeatsProps) {
  return (
    <section className="added-picks granted-feats" aria-label="Granted feats section">
      <h3 className="pick-group-label">Granted feats</h3>
      <ul className="added-list" aria-label="Granted feats">
        {feats.map((f) => (
          <li key={f.id}>
            <div className="pick-chosen added-line">
              <Icon name="lock" />
              <button type="button" className="pick-chosen-name added-name" onClick={() => onDescribe(f.id)}>{f.label}</button>
              {f.grantedBy && <span className="pick-chosen-src added-src">from {f.grantedBy}</span>}
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}
