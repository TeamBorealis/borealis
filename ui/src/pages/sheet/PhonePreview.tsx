import { useEffect, useState } from "react";
import type { SheetPaper, SheetPreview, SheetShare } from "../../api/contract";
import { Button, Modal, useToast } from "../../components";
import { downloadBase64 } from "../../lib/files";
import { phoneOs } from "../../lib/phone";
import { shareBase64File } from "../../lib/webShare";
import { useApi } from "../../state/ApiContext";
import { errorMessage } from "../../state/useAsync";
import { framePreviewHtml } from "./previewFrame";

export const PREVIEW_TITLE = "Character sheet preview";
const SHARE_TITLE = "Character sheet";

export interface PhonePreviewProps {
  characterId: string;
  preview: SheetPreview | null;
  loading: boolean;
  error: string | null;
  /** Request options for exports and shares (the device region's paper). */
  options: { defaultPaper: SheetPaper };
  onSave: () => void;
  onClose: () => void;
}

/**
 * TV2 C11/C16 (B-144): on phones the sheet preview opens full-screen, the only scroller on screen, with Save, Share and
 * Close at the bottom (U9a O5). Android shares from the backend; elsewhere the PDF is fetched (`shareSheet`, which answers
 * the bytes there) as the sheet opens, so Share can hand it to the web view's share sheet within the tap. Save is the system
 * save picker (B-148, `onSave`).
 */
export function PhonePreview({ characterId, preview, loading, error, options, onSave, onClose }: PhonePreviewProps) {
  const api = useApi();
  const toast = useToast();
  const android = phoneOs() === "android";
  const [pdf, setPdf] = useState<SheetShare | null>(null);
  const [sharing, setSharing] = useState(false);

  useEffect(() => {
    if (android || !preview) return;
    let live = true;
    api.shareSheet({ characterId, options: { format: "pdf", ...options } })
      .then((r) => { if (live && r.dataBase64) setPdf(r); })
      .catch((e: unknown) => { if (live) toast.push(errorMessage(e), "error"); });
    return () => { live = false; };
  }, [android, api, characterId, options, preview, toast]);

  const shareAndroid = async () => {
    setSharing(true);
    try { await api.shareSheet({ characterId, options: { format: "pdf", ...options } }); }
    catch (e) { toast.push(errorMessage(e), "error"); }
    finally { setSharing(false); }
  };
  const shareHere = async (r: SheetShare) => {
    try {
      const done = await shareBase64File(r.fileName, r.mime, r.dataBase64 ?? "", SHARE_TITLE);
      if (done === "unsupported") { downloadBase64(r.fileName, r.mime, r.dataBase64 ?? ""); toast.push(`Exported ${r.fileName}`); }
    } catch (e) { toast.push(errorMessage(e), "error"); }
  };
  const share = () => { if (android) void shareAndroid(); else if (pdf) void shareHere(pdf); };
  const canShare = !!preview && (android ? !sharing : !!pdf);

  const footer = (
    <>
      <Button variant="primary" onClick={onSave} disabled={!preview}>Save</Button>
      <Button onClick={share} disabled={!canShare}>Share</Button>
      <Button onClick={onClose}>Close</Button>
    </>
  );
  return (
    <Modal title={PREVIEW_TITLE} onClose={onClose} full className="v4-sheet-full" footer={footer}>
      {preview && !loading ? <iframe title={PREVIEW_TITLE} sandbox="" srcDoc={framePreviewHtml(preview.html)} />
        : <p className={`v4-sheet-empty ${error ? "err" : ""}`} role={error ? "alert" : "status"}>{error ?? "Generating..."}</p>}
    </Modal>
  );
}
