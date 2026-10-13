import { Alert, Button } from "../../components";
import type { AppSettings, DataDirs } from "../../api/contract";
import { DescCheck, FieldLabel, Help, PathBox, SectionDesc, SectionTitle } from "./rows";

/** Folders chosen in this session but not saved yet. A pick holds the backend's single-use token. */
export interface HeldPicks {
  documents?: { token: string; path: string };
  secondary?: { token: string; path: string } | "unlink";
}

/** DATA FILES: documents directory, automatic updates, secondary content directory and the cache button. */
export function DataFilesTab({ form, set, dirs, held, browse, unlink, clearCache, cacheCleared, error }: {
  form: AppSettings; set: (patch: Partial<AppSettings>) => void; dirs: DataDirs | null; held: HeldPicks;
  browse: (which: "documents" | "secondary") => void; unlink: () => void; clearCache: () => void; cacheCleared: boolean; error: string | null;
}) {
  const documents = held.documents ? held.documents.path : dirs && !dirs.isDefault ? dirs.documentsDir : "";
  const secondary = held.secondary === "unlink" ? "" : held.secondary ? held.secondary.path : dirs?.secondaryContentDir ?? "";
  return (
    <>
      <h3 className="aur-set-title13 first flush">DATA FILES</h3>
      <SectionDesc>Your data directories and additional content settings.</SectionDesc>
      <FieldLabel>DOCUMENTS DIRECTORY</FieldLabel>
      <PathBox label="Documents directory" value={documents} placeholder={dirs?.defaultDocumentsDir} onBrowse={() => browse("documents")} />
      <Help>The directory where your characters and gallery images are stored.</Help>
      <SectionTitle>ADDITIONAL CONTENT</SectionTitle>
      <SectionDesc>Manage additional content added through index files.</SectionDesc>
      <DescCheck spaced title="Automatic Updates" description="Check for updates to additional content at startup." checked={form.autoContentUpdate} onChange={(v) => set({ autoContentUpdate: v })} />
      <DescCheck spaced title="Content Import Notice" description="Remind me to import only content I own before each content import." checked={form.showImportNotice} onChange={(v) => set({ showImportNotice: v })} />
      <FieldLabel>SECONDARY CONTENT DIRECTORY</FieldLabel>
      <PathBox label="Secondary content directory" value={secondary} onBrowse={() => browse("secondary")} />
      <Help>A separate directory where custom files can be loaded from. An ideal place for your own custom content.</Help>
      <div className="aur-set-right">
        <Button variant="clear" icon="folder-remove" disabled={!secondary} onClick={unlink}>Unlink secondary directory</Button>
      </div>
      <div className="aur-set-left-clear">
        <Button disabled={cacheCleared} onClick={clearCache}>Clear Cache</Button>
      </div>
      {error && <Alert tone="danger" role="alert">{error}</Alert>}
    </>
  );
}
