import { useCallback, useEffect, useState } from "react";
import type { SheetPreview } from "../../api/contract";
import { Link } from "react-router";
import { Alert, ShellSubTabs } from "../../components";
import "./sheet.css";
import { RequireCharacter } from "../../components/RequireCharacter";
import { downloadBase64 } from "../../lib/files";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useSettings } from "../../state/SettingsContext";
import { errorMessage } from "../../state/useAsync";
import { useShell, useToast } from "../../components";
import { DetailField } from "../manage/DetailField";
import { mdiFileFind } from "@mdi/js";
import { MgIcon } from "../manage/mgIcons";
import { framePreviewHtml } from "./previewFrame";
import { displayPath } from "../../lib/displayPath";
import { attunementWarnings } from "../equipment/attunement";

function Body() {
  const api = useApi();
  const toast = useToast();
  const { openSettings } = useShell();
  const { snapshot } = useCharacter();
  const { settings } = useSettings();
  const [preview, setPreview] = useState<SheetPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const id = snapshot?.id;
  const revision = snapshot?.dirty ? snapshot.name + snapshot.level.total : snapshot?.name;

  const generate = useCallback(async () => {
    if (!id) return;
    setLoading(true);
    try {
      setPreview(await api.renderSheet({ characterId: id }));
      setError(null);
    } catch (e) {
      setError(`Unable to generate a preview of your character sheet. (${errorMessage(e)})`);
    } finally { setLoading(false); }
  }, [api, id]);

  useEffect(() => { if (settings.sheet.autoGenerate) void generate(); }, [settings.sheet.autoGenerate, generate, revision]);

  const exportSheet = async () => {
    if (!id) return;
    try {
      const r = await api.exportSheet({ characterId: id, options: { format: "pdf" } });
      if (r.dataBase64) downloadBase64(r.fileName, r.mime, r.dataBase64);
      toast.push(r.path ? `Saved to ${displayPath(r.path)}` : `Exported ${r.fileName}`);
    } catch (e) { toast.push(errorMessage(e), "error"); }
  };
  const s = settings.sheet;
  return (
    <>
    <ShellSubTabs><div className="tabs sub"><span className="tab" aria-current="page">Character Sheet</span></div></ShellSubTabs>
    <div className="v4-sheet">
      <div className="v4-sheet-left">
        <h2>Generate preview</h2>
        <p className="lead">Build a preview of your sheet here, or write it straight to a PDF file. The app settings let you decide whether the form fields can be typed into, and whether spell, item, attack and feature cards are added after the sheet pages.</p>
        <div className="v4-sheet-link"><Link to="/settings?tab=sheet" onClick={(e) => { e.preventDefault(); openSettings("sheet"); }}><MgIcon name="cog" />Character sheet settings</Link></div>
        <div className="v4-sheet-btns">
          <button type="button" className="sh-btn" style={{ padding: "0 3px 0 6px", gap: 7 }} onClick={() => void generate()} disabled={loading}><svg className="icon" viewBox="6 2 12 20" style={{ width: 13, height: 17 }} aria-hidden focusable="false"><path d={mdiFileFind} /></svg>{loading ? "Generating..." : "Generate Preview"}</button>
          <button type="button" className="sh-btn" style={{ marginLeft: -1, paddingLeft: 6, paddingRight: 5 }} onClick={() => void exportSheet()}><MgIcon name="pdf" />Save Character Sheet</button>
        </div>
        {snapshot && attunementWarnings(snapshot).map((w) => <Alert key={w} tone="warning">{w}</Alert>)}
        <div className="v4-sheet-sec">
          <h2>Additional fields</h2>
          <p className="lead">A few sheet fields cannot be filled in for you yet. Whatever you type below appears on the sheet unchanged.</p>
          <DetailField k="sheetAcNote" label="Armor class (conditional field)" area ok />
        </div>
      </div>
      <div className="v4-sheet-pane">
        {preview ? <iframe title="Character sheet preview" sandbox="" srcDoc={framePreviewHtml(preview.html)} />
          : <p className={`v4-sheet-empty ${error ? "err" : ""}`} role={error ? "alert" : undefined}>{error ?? `No preview yet. Press Generate Preview to see your sheet (auto-generate is ${s.autoGenerate ? "on" : "off"}).`}</p>}
        {preview && error && <Alert tone="danger">{error}</Alert>}
      </div>
    </div>
    </>
  );
}

export function SheetPage() {
  return <RequireCharacter><Body /></RequireCharacter>;
}
