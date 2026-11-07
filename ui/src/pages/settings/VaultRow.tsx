import type { DataDirs } from "../../api/contract";
import { Alert, Button } from "../../components";
import { VAULT_UNAVAILABLE } from "../../components/shell/useFolderNotice";
import type { HeldPicks } from "./DataFilesTab";
import { FieldLabel, Help, PathBox } from "./rows";

/**
 * CONTENT > OBSIDIAN VAULT (GitHub #15): the folder the Notes section opens in place. Any folder of Markdown notes works; a
 * `.obsidian` folder is optional. Browse and Clear wait for Save, like the portrait library and the DATA FILES folders. Desktop
 * only for now: phones have no folder picker.
 */
export function VaultRow({ dirs, held, browse, clear }: {
  dirs: DataDirs | null; held: HeldPicks["vault"]; browse: () => void; clear: () => void;
}) {
  const path = held === "clear" ? "" : held ? held.path : dirs?.vaultDir ?? "";
  const offline = !held && !!dirs?.vaultUnavailable;
  return (
    <>
      <FieldLabel>OBSIDIAN VAULT</FieldLabel>
      <PathBox label="Obsidian vault" value={path} onBrowse={browse} />
      <Help>Your Obsidian vault, or any folder of Markdown notes. Notes opens it in place: notes are read and saved where they are, and nothing under .obsidian is changed.</Help>
      {offline && <Alert tone="warning" role="status">{VAULT_UNAVAILABLE} Clear it to stop using it.</Alert>}
      <div className="aur-set-right">
        <Button variant="clear" icon="folder-remove" disabled={!path} onClick={clear}>Clear vault</Button>
      </div>
    </>
  );
}
