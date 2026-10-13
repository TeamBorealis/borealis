import { useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ConversionItem, ConversionOutcome, ConversionPreview, ConversionStats } from "../../api/contract";
import { Alert, Button, Chip, Modal, signed } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useAsync } from "../../state/useAsync";
import { errorMessage } from "../../state/useAsync";

const GROUPS: { id: ConversionOutcome; title: string; tone: "success" | "info" | "danger" | "warning" }[] = [
  { id: "needs_choice", title: "Needs your choice", tone: "warning" },
  { id: "kept", title: "Kept", tone: "success" },
  { id: "mapped", title: "Mapped to 2024", tone: "info" },
  { id: "dropped", title: "Dropped", tone: "danger" },
];

function itemNote(i: ConversionItem): string {
  if (i.outcome === "dropped") return i.reason ?? "";
  if (i.outcome === "needs_choice") return i.what ?? "";
  if (i.outcome === "mapped") return i.toId ? `becomes ${i.toId}` : "";
  return i.crossEdition ? "stays as a 2014 element" : "";
}

function StatsTable({ before, after }: { before: ConversionStats; after: ConversionStats }) {
  const rows: [string, string, string][] = [
    ["Hit points", String(before.hitPoints), String(after.hitPoints)],
    ["Armor Class", String(before.armorClass), String(after.armorClass)],
    ["Initiative", signed(before.initiative), signed(after.initiative)],
    ["Proficiency", signed(before.proficiencyBonus), signed(after.proficiencyBonus)],
    ["Speed", `${before.speed} ft`, `${after.speed} ft`],
    ["Ability scores", before.abilityScores.join(" / "), after.abilityScores.join(" / ")],
    ...(before.spellSaveDc !== null || after.spellSaveDc !== null ? [["Spell save DC", String(before.spellSaveDc ?? "-"), String(after.spellSaveDc ?? "-")] as [string, string, string]] : []),
  ];
  return (
    <table className="compare" aria-label="Before and after">
      <thead><tr><th scope="col">Stat</th><th scope="col">2014</th><th scope="col">2024</th></tr></thead>
      <tbody>{rows.map(([k, b, a]) => <tr key={k} className={b !== a ? "changed" : ""}><th scope="row">{k}</th><td>{b}</td><td>{a}</td></tr>)}</tbody>
    </table>
  );
}

function PreviewBody({ preview }: { preview: ConversionPreview }) {
  return (
    <div className="stack">
      <p>Creates a new character, <strong>{preview.name}</strong>. The original is not changed.</p>
      <StatsTable before={preview.before} after={preview.after} />
      {GROUPS.map((g) => {
        const items = preview.items.filter((i) => i.outcome === g.id);
        if (items.length === 0) return null;
        return (
          <section key={g.id} aria-label={g.title} className="stack">
            <h3>{g.title} <Chip tone={g.tone}>{items.length}</Chip></h3>
            <ul className="list">
              {items.map((i, n) => (
                <li key={n} className="resource-row"><span className="grow"><strong>{i.label}</strong> <span className="muted small">{i.category.replace(/_/g, " ")}</span>{itemNote(i) && <span className="small" style={{ display: "block" }}>{itemNote(i)}</span>}</span></li>
              ))}
            </ul>
          </section>
        );
      })}
    </div>
  );
}

export function ConvertDialog({ characterId, onClose }: { characterId: string; onClose: () => void }) {
  const api = useApi();
  const nav = useNavigate();
  const ch = useCharacter();
  const preview = useAsync(() => api.previewConvertTo2024({ characterId }), [api, characterId]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const apply = async () => {
    setBusy(true);
    try {
      const done = await api.applyConvertTo2024({ characterId });
      ch.adopt(done.snapshot);
      onClose();
      nav("/builder");
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };
  return (
    <Modal wide title="Convert to 2024" onClose={onClose} footer={<><Button onClick={onClose}>Cancel</Button><Button variant="primary" disabled={!preview.data || busy} onClick={() => void apply()}>{busy ? "Converting..." : "Apply"}</Button></>}>
      {preview.loading && <p className="muted">Checking what would carry over...</p>}
      {(preview.error || error) && <Alert tone="danger">{preview.error ?? error}</Alert>}
      {preview.data && <PreviewBody preview={preview.data} />}
    </Modal>
  );
}
