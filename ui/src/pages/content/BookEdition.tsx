import { useState } from "react";
import type { ContentSource, Ruleset } from "../../api/contract";
import { useToast } from "../../components";
import { ComboBox } from "../../components/controls";
import { useApi } from "../../state/ApiContext";
import { setStatus } from "../../state/status";
import { errorMessage } from "../../state/useAsync";

export const EDITION_OPTIONS: { value: Ruleset; label: string }[] = [
  { value: "dnd2014", label: "2014 rules" },
  { value: "dnd2024", label: "2024 rules" },
];
const year = (r: Ruleset): string => (r === "dnd2014" ? "2014" : "2024");

/**
 * IM14 (GitHub #11): the rules edition of the selected book on the Sources page. An imported book's edition can be set; the
 * choice is stored (not written into the book's files) and reloads the content. Characters that already use the book keep
 * its content; new characters follow the edition. Bundled books show theirs.
 */
export function BookEdition({ source, onChanged }: { source: ContentSource; onChanged: (sources: ContentSource[]) => void }) {
  const api = useApi();
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  if (!source.editionEditable) {
    return <p className="v4-src-edition">Rules edition: {source.edition ? year(source.edition) : "2014 and 2024"} (bundled book)</p>;
  }
  const change = async (edition: Ruleset) => {
    setBusy(true);
    try {
      onChanged(await api.setSourceEdition({ sourceId: source.id, edition }));
      setStatus(`${source.name} now uses the ${year(edition)} rules.`);
    } catch (e) { toast.push(errorMessage(e), "error"); } finally { setBusy(false); }
  };
  return (
    <div className="v4-src-edition">
      <label htmlFor="v4-src-edition">Rules edition of this book</label>
      <ComboBox id="v4-src-edition" options={EDITION_OPTIONS} value={source.edition ?? "dnd2014"} disabled={busy}
        onChange={(e) => void change(e.target.value as Ruleset)} />
      <p className="sub">
        {source.editionSetByUser ? "Set by you." : "Found in the book, or guessed from its name and date."}
        {" "}Characters that already use this book keep its content; new characters follow this edition.
      </p>
    </div>
  );
}
