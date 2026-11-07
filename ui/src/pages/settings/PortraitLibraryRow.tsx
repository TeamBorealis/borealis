import type { DataDirs } from "../../api/contract";
import { Alert, Button } from "../../components";
import { PORTRAIT_LIBRARY_UNAVAILABLE } from "../../components/shell/useFolderNotice";
import type { HeldPicks } from "./DataFilesTab";
import { FieldLabel, Help, PathBox } from "./rows";

/**
 * CONTENT > PORTRAIT LIBRARY (GitHub #32): the folder whose images the portraits gallery shows, like the classic builder's
 * `<documents>/portraits` folder. Browse and Clear wait for Save, like the DATA FILES folders. Desktop only: phones have no
 * folder picker.
 */
export function PortraitLibraryRow({ dirs, held, browse, clear }: {
  dirs: DataDirs | null; held: HeldPicks["portraits"]; browse: () => void; clear: () => void;
}) {
  const path = held === "clear" ? "" : held ? held.path : dirs?.portraitLibraryDir ?? "";
  const offline = !held && !!dirs?.portraitLibraryUnavailable;
  return (
    <>
      <FieldLabel>PORTRAIT LIBRARY</FieldLabel>
      <PathBox label="Portrait library" value={path} onBrowse={browse} />
      <Help>A folder of your own portrait images (PNG, JPEG, GIF or WebP). The portraits gallery shows them, with the images one sub-folder down.</Help>
      {offline && <Alert tone="warning" role="status">{PORTRAIT_LIBRARY_UNAVAILABLE} Clear it to stop using it.</Alert>}
      <div className="aur-set-right">
        <Button variant="clear" icon="folder-remove" disabled={!path} onClick={clear}>Clear portrait library</Button>
      </div>
    </>
  );
}
