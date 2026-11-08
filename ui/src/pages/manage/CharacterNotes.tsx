import { useState } from "react";
import { useNavigate } from "react-router";
import { isApiError, type VaultEntry } from "../../api/contract";
import { Alert, Button, useShell } from "../../components";
import { VAULT_UNAVAILABLE } from "../../components/shell/useFolderNotice";
import { notesOnThisDevice } from "../../lib/phone";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useSettings } from "../../state/SettingsContext";
import { setStatus } from "../../state/status";
import { errorMessage } from "../../state/useAsync";
import { LinkPickerDialog } from "../notes/NoteDialogs";
import { displayName } from "../notes/noteNames";
import { MgSection } from "./Field";

/** What Notes opens to show a note (`location.state`). */
export interface OpenNoteState { openNote: string }

export const NOTES_INTRO = "Keep this character's backstory and session notes in your Obsidian vault.";
export const NO_VAULT = "Set a vault in Settings > Content to link this character to a note.";

/** Why the linked note cannot be opened: it is gone, or the vault cannot be read. Both offer to link another note. */
interface Problem { missing: boolean; message: string }

function problemOf(e: unknown, path: string): Problem {
  if (isApiError(e) && e.code === "not_found") {
    return { missing: true, message: `The note '${displayName(path)}' is no longer in your vault. Link another note, or create a new one.` };
  }
  return { missing: false, message: errorMessage(e) };
}

/** iCloud is still downloading the note: Notes shows it when it arrives. */
const downloading = (e: unknown): boolean => isApiError(e) && (e.details as { downloading?: boolean } | undefined)?.downloading === true;

/**
 * B-222 (decision U17b): the character's note in the Obsidian vault, on Manage > Character. Link an existing note (a searchable
 * picker over the vault's notes), create one from the character (front matter and headings, linked at once), or open the linked
 * note in Start > Notes. The link is the vault-relative path saved in the character file. Without a vault the section says where
 * to set one; a missing note or an unreadable vault shows a message and offers to link again.
 */
export function CharacterNotes() {
  const api = useApi();
  const navigate = useNavigate();
  const { openSettings } = useShell();
  const { settings } = useSettings();
  const { snapshot, mutate } = useCharacter();
  const [picking, setPicking] = useState<readonly VaultEntry[] | null>(null);
  const [problem, setProblem] = useState<Problem | null>(null);
  if (!snapshot || !notesOnThisDevice()) return null;
  const linked = snapshot.vaultNote;

  const open = async () => {
    if (!linked) return;
    try {
      await api.vaultRead({ path: linked });
    } catch (e) {
      if (!downloading(e)) { setProblem(problemOf(e, linked)); return; }
    }
    setProblem(null);
    navigate("/notes", { state: { openNote: linked } satisfies OpenNoteState });
  };
  const startLink = async () => {
    try {
      const tree = await api.vaultTree();
      if (!tree.dir || tree.unavailable) { setProblem({ missing: false, message: tree.dir ? VAULT_UNAVAILABLE : NO_VAULT }); return; }
      setPicking(tree.entries.filter((e) => e.kind === "note"));
    } catch (e) {
      setProblem({ missing: false, message: errorMessage(e) });
    }
  };
  const link = async (path: string) => {
    setPicking(null);
    const next = await mutate((a, cid) => a.setCharacterNote({ characterId: cid, path }));
    if (next) { setProblem(null); setStatus(`Linked to the note '${displayName(path)}'.`); }
  };
  const create = async () => {
    const next = await mutate((a, cid) => a.vaultCreateCharacterNote({ characterId: cid }));
    if (next?.vaultNote) { setProblem(null); setStatus(`Created the note '${displayName(next.vaultNote)}'.`); }
  };
  const unlink = async () => {
    if (await mutate((a, cid) => a.setCharacterNote({ characterId: cid, path: null }))) setProblem(null);
  };

  return (
    <MgSection title="Notes" sub={NOTES_INTRO} className="mg-gap-s mg-char-notes">
      {settings.vaultDir === null ? (
        <div className="mg-note">
          <span className="mg-hint">{NO_VAULT}</span>
          <div className="mg-actions"><Button onClick={() => openSettings("content")}>Open Settings</Button></div>
        </div>
      ) : (
        <>
          <div className="mg-note">
            <span className={`mg-note-name${linked ? "" : " none"}`} title={linked ?? undefined}>{linked ? displayName(linked) : "No note linked"}</span>
            <div className="mg-actions">
              {linked && <Button onClick={() => void open()}>Open note</Button>}
              <Button onClick={() => void startLink()}>{linked ? "Link another note..." : "Link note..."}</Button>
              {(!linked || problem?.missing) && <Button onClick={() => void create()}>Create note</Button>}
              {linked && <Button onClick={() => void unlink()}>Unlink</Button>}
            </div>
          </div>
          {problem && <Alert tone="warning" role="alert">{problem.message}</Alert>}
        </>
      )}
      {picking && <LinkPickerDialog title="Link a note to this character" entries={picking} onPick={(p) => void link(p)} onClose={() => setPicking(null)} />}
    </MgSection>
  );
}
