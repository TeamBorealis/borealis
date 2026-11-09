import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { SheetPreview } from "../../api/contract";
import { Link, useSearchParams } from "react-router";
import { Alert, ShellSubTabs } from "../../components";
import "./sheet.css";
import { RequireCharacter } from "../../components/RequireCharacter";
import { downloadBase64 } from "../../lib/files";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { useSettings } from "../../state/SettingsContext";
import { errorMessage } from "../../state/useAsync";
import { GuideLink, useShell, useToast } from "../../components";
import { DetailField } from "../manage/DetailField";
import { mdiFileFind } from "@mdi/js";
import { MgIcon } from "../manage/mgIcons";
import { framePreviewHtml } from "./previewFrame";
import { regionPaper, deviceLanguages } from "../../lib/paper";
import { usePhone } from "../../lib/phone";
import { useNarrow } from "../../components/shell/useNarrow";
import { PhonePreview, PREVIEW_TITLE } from "./PhonePreview";
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
  const [phoneOpen, setPhoneOpen] = useState(false);
  const phone = usePhone();
  const narrow = useNarrow();
  const pane = useRef<HTMLDivElement>(null);
  const id = snapshot?.id;
  const revision = snapshot?.dirty ? snapshot.name + snapshot.level.total : snapshot?.name;
  /** The device region's paper, used while the Paper setting is unset (TV1, B-147). */
  const options = useMemo(() => ({ defaultPaper: regionPaper(deviceLanguages()) }), []);

  const generate = useCallback(async (): Promise<boolean> => {
    if (!id) return false;
    setLoading(true);
    try {
      setPreview(await api.renderSheet({ characterId: id, options }));
      setError(null);
      return true;
    } catch (e) {
      setError(`Unable to generate a preview of your character sheet. (${errorMessage(e)})`);
      return false;
    } finally { setLoading(false); }
  }, [api, id, options]);

  // Phones show the preview only full-screen (TV2 C11/C16), so nothing is generated until Generate Preview is tapped.
  useEffect(() => { if (settings.sheet.autoGenerate && !phone) void generate(); }, [settings.sheet.autoGenerate, generate, revision, phone]);

  /** B-144: the result of a tap is always in sight: full-screen on phones, the pane scrolled into view on a narrow tablet. */
  const generateNow = async () => {
    if (phone) setPhoneOpen(true);
    const ok = await generate();
    if (ok && narrow && !phone) pane.current?.scrollIntoView({ block: "start", behavior: "smooth" });
  };

  // B-143: the status bar's View sheet (`?preview=1`) opens the sheet with its preview generated, as Generate Preview does.
  const [params, setParams] = useSearchParams();
  const wantsPreview = params.get("preview") === "1";
  const runPreview = useRef(generateNow);
  runPreview.current = generateNow;
  useEffect(() => {
    if (!wantsPreview || !id) return;
    setParams((p) => { const next = new URLSearchParams(p); next.delete("preview"); return next; }, { replace: true });
    void runPreview.current();
  }, [wantsPreview, id, setParams]);

  const exportSheet = async () => {
    if (!id) return;
    try {
      const r = await api.exportSheet({ characterId: id, options: { format: "pdf", ...options } });
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
        <div className="v4-sheet-head"><h2>Generate preview</h2><GuideLink guide="print-character-sheet" /></div>
        <p className="lead">Build a preview of your sheet here, or write it straight to a PDF file. The app settings let you decide whether the form fields can be typed into, and whether spell, item, attack and feature cards are added after the sheet pages.</p>
        <div className="v4-sheet-link"><Link to="/settings?tab=sheet" onClick={(e) => { e.preventDefault(); openSettings("sheet"); }}><MgIcon name="cog" />Character sheet settings</Link></div>
        <div className="v4-sheet-btns">
          <button type="button" className="sh-btn" style={{ padding: "0 3px 0 6px", gap: 7 }} onClick={() => void generateNow()} disabled={loading}><svg className="icon" viewBox="6 2 12 20" style={{ width: 13, height: 17 }} aria-hidden focusable="false"><path d={mdiFileFind} /></svg>
            {/* Both labels share one grid cell, so the button keeps its width and Save does not move (B-144). */}
            <span className="sh-lbl"><span aria-hidden={loading ? "true" : undefined}>Generate Preview</span><span aria-hidden={loading ? undefined : "true"}>Generating...</span></span></button>
          <button type="button" className="sh-btn" style={{ marginLeft: -1, paddingLeft: 6, paddingRight: 5 }} onClick={() => void exportSheet()}><MgIcon name="pdf" />Save Character Sheet</button>
        </div>
        {snapshot && attunementWarnings(snapshot).map((w) => <Alert key={w} tone="warning">{w}</Alert>)}
        <div className="v4-sheet-sec">
          <h2>Additional fields</h2>
          <p className="lead">A few sheet fields cannot be filled in for you yet. Whatever you type below appears on the sheet unchanged.</p>
          <DetailField k="sheetAcNote" label="Armor class (conditional field)" area ok />
        </div>
      </div>
      {!phone && (
        <div className="v4-sheet-pane" ref={pane}>
          {preview ? <iframe title={PREVIEW_TITLE} sandbox="" srcDoc={framePreviewHtml(preview.html)} />
            : <p className={`v4-sheet-empty ${error ? "err" : ""}`} role={error ? "alert" : undefined}>{error ?? `No preview yet. Press Generate Preview to see your sheet (auto-generate is ${s.autoGenerate ? "on" : "off"}).`}</p>}
          {preview && error && <Alert tone="danger">{error}</Alert>}
        </div>
      )}
    </div>
    {phone && phoneOpen && id && (
      <PhonePreview characterId={id} preview={preview} loading={loading} error={error} options={options} onSave={() => void exportSheet()} onClose={() => setPhoneOpen(false)} />
    )}
    </>
  );
}

export function SheetPage() {
  return <RequireCharacter><Body /></RequireCharacter>;
}
