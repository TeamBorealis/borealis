import { useNavigate } from "react-router";
import type { Ruleset } from "../../api/contract";
import { useConfirm, useToast } from "../../components";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { setStatus } from "../../state/status";
import { errorMessage } from "../../state/useAsync";
import { editionYear } from "./EmptyListNote";

const SKIP_SAVE = "skip-save";

/**
 * "Start over as 2014" (U14, GitHub #10). Classes never cross editions (R4), so the safe way out is a NEW character of the
 * other edition: after a confirmation the backend creates it with the same name, details, options, level, scores and
 * sources, and opens it. The old character's file is never rewritten by this; unsaved changes are saved only when the user
 * picks "Save and start over".
 */
export function useStartOver(): (edition: Ruleset) => Promise<void> {
  const api = useApi();
  const ch = useCharacter();
  const confirm = useConfirm();
  const toast = useToast();
  const nav = useNavigate();
  return async (edition) => {
    const cur = ch.snapshot;
    if (!cur) return;
    const year = editionYear(edition);
    const oldYear = editionYear(cur.ruleset);
    const body = (
      <>
        <p>
          Borealis makes a new {year} character named {cur.name}, with the same details, options, level and sources, and opens it.
          {" "}Your {oldYear} character {cur.name} stays in your collection, unchanged.
        </p>
        {cur.dirty && <p>{cur.name} has unsaved changes. Save them first, or leave the saved copy as it is.</p>}
      </>
    );
    const answer = await confirm({
      title: `Start over as ${year}?`, body,
      confirmLabel: cur.dirty ? "Save and start over" : `Start over as ${year}`,
      ...(cur.dirty ? { extra: { label: "Start over without saving", value: SKIP_SAVE } } : {}),
    });
    if (answer !== "confirm" && answer !== SKIP_SAVE) return;
    try {
      if (cur.dirty && answer === "confirm") await api.saveCharacter({ characterId: cur.id });
      const snap = await api.startOverInEdition({ characterId: cur.id, ruleset: edition });
      ch.adopt(snap);
      nav("/builder/class");
      setStatus(`Started ${snap.name} over as a ${year} character. The ${oldYear} character is still in your collection.`);
    } catch (e) { toast.push(errorMessage(e), "error"); }
  };
}
