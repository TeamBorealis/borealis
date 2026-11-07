import { Alert, Button } from "../../components";
import type { AppSettings, DataDirs } from "../../api/contract";
import { LogsRow } from "./LogsRow";
import { DescCheck, FieldLabel, Help, PathBox, SectionDesc, SectionTitle } from "./rows";
import { PhoneCase } from "../../lib/phoneCase";
import { SECONDARY_UNAVAILABLE, documentsUnavailableText } from "../../components/shell/useFolderNotice";

/** Folders chosen in this session but not saved yet. A pick holds the backend's single-use token. */
export interface HeldPicks {
  documents?: { token: string; path: string };
  secondary?: { token: string; path: string } | "unlink";
  /** Settings > CONTENT portrait library (GitHub #32): a new folder, or "clear" for none. */
  portraits?: { token: string; path: string } | "clear";
  /** Settings > CONTENT Obsidian vault (GitHub #15): a new folder, or "clear" for none. */
  vault?: { token: string; path: string } | "clear";
}

/** Which folder a Browse button picks. */
export type HeldFolder = keyof HeldPicks;

/** The path of a held pick, if it is a folder (not an unlink / clear). */
export const heldPath = (pick: HeldPicks[HeldFolder]): string | undefined => (typeof pick === "object" ? pick.path : undefined);

/** DATA FILES: documents directory, automatic updates, secondary content directory and the cache button. */
export function DataFilesTab({ form, set, dirs, held, browse, unlink, clearCache, cacheCleared, error }: {
  form: AppSettings; set: (patch: Partial<AppSettings>) => void; dirs: DataDirs | null; held: HeldPicks;
  browse: (which: HeldFolder) => void; unlink: () => void; clearCache: () => void; cacheCleared: boolean; error: string | null;
}) {
  const offlineDocs = held.documents ? null : dirs?.documentsDirUnavailable ?? null;
  const documents = held.documents ? held.documents.path : offlineDocs ?? (dirs && !dirs.isDefault ? dirs.documentsDir : "");
  const secondary = held.secondary === "unlink" ? "" : held.secondary ? held.secondary.path : dirs?.secondaryContentDir ?? "";
  const offlineSecondary = !held.secondary && !!dirs?.secondaryContentUnavailable;
  return (
    <>
      <h3 className="aur-set-title13 first flush"><PhoneCase>DATA FILES</PhoneCase></h3>
      <SectionDesc>Your data directories and additional content settings.</SectionDesc>
      <FieldLabel>DOCUMENTS DIRECTORY</FieldLabel>
      <PathBox label="Documents directory" value={documents} placeholder={dirs?.defaultDocumentsDir} onBrowse={() => browse("documents")} />
      <Help>The directory where your characters and gallery images are stored.</Help>
      {offlineDocs && <Alert tone="warning" role="status">{documentsUnavailableText(offlineDocs)}</Alert>}
      <SectionTitle>ADDITIONAL CONTENT</SectionTitle>
      <SectionDesc>Manage additional content added through index files.</SectionDesc>
      <DescCheck spaced title="Automatic Updates" description="Check for updates to additional content at startup." checked={form.autoContentUpdate} onChange={(v) => set({ autoContentUpdate: v })} />
      <DescCheck spaced title="Content Import Notice" description="Remind me to import only content I own before each content import." checked={form.showImportNotice} onChange={(v) => set({ showImportNotice: v })} />
      <FieldLabel>SECONDARY CONTENT DIRECTORY</FieldLabel>
      <PathBox label="Secondary content directory" value={secondary} onBrowse={() => browse("secondary")} />
      <Help>A separate directory where custom files can be loaded from. An ideal place for your own custom content.</Help>
      {offlineSecondary && <Alert tone="warning" role="status">{SECONDARY_UNAVAILABLE} Unlink it to remove that content.</Alert>}
      <div className="aur-set-right">
        <Button variant="clear" icon="folder-remove" disabled={!secondary} onClick={unlink}>Unlink secondary directory</Button>
      </div>
      <div className="aur-set-left-clear">
        <Button disabled={cacheCleared} onClick={clearCache}>Clear Cache</Button>
      </div>
      <LogsRow mobile={form.mobile} />
      {error && <Alert tone="danger" role="alert">{error}</Alert>}
    </>
  );
}
