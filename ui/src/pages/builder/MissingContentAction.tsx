import { useNavigate } from "react-router";
import { useCharacter } from "../../state/CharacterContext";
import { MissingContentDialog } from "../characters/MissingContentList";

/** GitHub #37: the notice's action that opens the list of what the import notice counts. */
export function ShowMissingContentButton({ onClick }: { onClick: () => void }) {
  return <>{" "}<button type="button" className="bp-linkbtn" onClick={onClick}>Show missing content</button></>;
}

/**
 * GitHub #37: the import notice counts the content the file names that is not installed; this lists it (ids, names, types, by
 * the source the ids name), with the way to add that content and re-check without re-importing. It stays open after a
 * re-check that finds everything, to say so.
 */
export function CharacterMissingContentDialog({ onClose }: { onClose: () => void }) {
  const ch = useCharacter();
  const nav = useNavigate();
  const snapshot = ch.snapshot;
  if (!snapshot) return null;
  const recheck = () => void ch.mutate(async (api, characterId) => {
    await api.recheckMissingContent();
    return api.getSnapshot({ characterId });
  });
  return (
    <MissingContentDialog entries={[{ name: snapshot.name, missing: snapshot.missingContent }]} rechecking={ch.busy}
      onClose={onClose} onOpenContent={() => { onClose(); nav("/content"); }} onRecheck={recheck} />
  );
}
