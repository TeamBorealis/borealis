import { useState } from "react";
import type { HomebrewKindDescriptor } from "../../api/contract";
import { Alert, Button, Expander, Flyout, TextBox } from "../../components";
import { errorMessage } from "../../state/useAsync";
import "../characters/characters.css";

/** Kinds offered by New element, in the CATEGORIES order: main forms, small types, then the expert form. Child kinds are
 * made inside their parent (a class's features). */
export function creatableKinds(kinds: readonly HomebrewKindDescriptor[]): HomebrewKindDescriptor[] {
  const order: Record<string, number> = { main: 0, small: 1, expert: 2 };
  return kinds.filter((k) => k.group !== "child").sort((a, b) => (order[a.group] ?? 3) - (order[b.group] ?? 3));
}

/**
 * New element (spec 13.11), in the New Character flyout's look: one dashed option card per kind with its one-line
 * explanation, the name field, CREATE / CANCEL.
 */
export function NewElementFlyout({ kinds, initialKind, onCreate, onClose }: {
  kinds: readonly HomebrewKindDescriptor[]; initialKind: string | null; onCreate: (kindId: string, name: string) => Promise<void>; onClose: () => void;
}) {
  const offered = creatableKinds(kinds);
  const [kindId, setKindId] = useState(initialKind ?? offered[0]?.id ?? "");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const create = async () => {
    if (!kindId) return;
    setBusy(true);
    try { await onCreate(kindId, name.trim()); } catch (e) { setError(errorMessage(e)); } finally { setBusy(false); }
  };
  return (
    <Flyout title="New element" onClose={onClose}
      footer={<div className="v4-foot"><Button data-testid="create-element" disabled={busy || !kindId} onClick={() => void create()}>Create</Button><Button onClick={onClose}>Cancel</Button></div>}>
      <form className="v4-new" onSubmit={(e) => { e.preventDefault(); void create(); }}>
        <Expander flyout title="Element">
          <p className="sub">Choose what to make, then give it a name. You can rename it at any time.</p>
          <label className="v4-fl" htmlFor="hb-new-name">Name</label>
          <TextBox light id="hb-new-name" value={name} placeholder="Unnamed" data-autofocus onChange={(e) => setName(e.target.value)} />
          {error && <Alert tone="danger" role="alert">{error}</Alert>}
        </Expander>
        <section className="aur-expander flyout-variant v4-plain" aria-labelledby="hb-kinds-title">
          <div className="aur-expander-head"><h2 className="aur-expander-toggle" id="hb-kinds-title"><span className="aur-expander-title">Kind</span></h2></div>
          <div className="aur-expander-body">
            <div className="v4-opts hb-kind-cards" role="radiogroup" aria-labelledby="hb-kinds-title">
              {offered.map((k) => (
                <button key={k.id} type="button" role="radio" aria-checked={k.id === kindId} className="v4-opt hb-kind-card" onClick={() => setKindId(k.id)}>
                  <span className="hb-kind-name">{k.label}</span>
                  <span className="hb-kind-desc">{k.description}</span>
                </button>
              ))}
            </div>
          </div>
        </section>
      </form>
    </Flyout>
  );
}
