import { useState } from "react";
import { Alert, Button } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useAsync } from "../../state/useAsync";
import { ConvertDialog } from "./ConvertDialog";
import { EdCheck, EdField, EdSection } from "./OptionsTab";

/** Edition section of the Edition flyout: ruleset, strict mode, compatibility report and Convert to 2024 (classic builder flyout metrics). */
export function RulesTab() {
  const { snapshot, mutate } = useCharacter();
  const api = useApi();
  const [converting, setConverting] = useState(false);
  const id = snapshot?.id ?? "";
  const report = useAsync(() => api.getCompatibilityReport({ characterId: id }), [api, id, snapshot?.play.compatibility.strict, snapshot?.play.compatibility.entries.length]);
  if (!snapshot) return null;
  const strict = report.data?.strict ?? snapshot.play.compatibility.strict;
  const entries = report.data?.entries ?? [];
  const is2014 = snapshot.ruleset === "dnd2014";
  return (
    <>
      <EdSection title="Edition" sub="Which rules this character follows, and whether content from the other edition may be picked.">
        <EdField label="Rules edition">{() => <p className="v4-ed-value">{is2014 ? "2014 rules" : "2024 rules"}</p>}</EdField>
        <EdCheck caps title="Strict mode" description="New picks only show content from this character's edition. Existing picks are never removed."
          checked={strict} onChange={(on) => void mutate((a, cid) => a.setAllowCrossEdition({ characterId: cid, allow: !on }))} />
        <div className="v4-ed-actions"><Button onClick={() => void mutate((a, cid) => a.setAllowCrossEdition({ characterId: cid, allow: null }))}>Reset to edition default</Button></div>
      </EdSection>
      <EdSection title="Compatibility report" sub="Elements on this character that come from another edition and how they were handled.">
        {report.error && <Alert tone="danger">{report.error}</Alert>}
        {entries.length === 0 ? <p className="v4-ed-value">Everything here is from this character's edition.</p> : (
          <ul className="v4-ed-list" aria-label="Compatibility report">
            {entries.map((e) => (
              <li key={e.element.id}>
                <span className="head">{e.element.name}{e.converted ? " (adapted)" : ""}</span>
                <span className="desc">{e.element.type} · from {e.from === "dnd2014" ? "2014" : "2024"}</span>
                {e.notes.map((n, i) => <span key={i} className="desc">{n}</span>)}
              </li>
            ))}
          </ul>
        )}
      </EdSection>
      {is2014 && (
        <EdSection title="Convert to 2024" sub="Preview what would carry over, then save the result as a new 2024 character. The original stays as it is.">
          <div className="v4-ed-actions"><Button onClick={() => setConverting(true)}>Convert to 2024…</Button></div>
        </EdSection>
      )}
      {converting && <ConvertDialog characterId={snapshot.id} onClose={() => setConverting(false)} />}
    </>
  );
}
