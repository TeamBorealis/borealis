import { Button, useToast } from "../../components";
import { useApi } from "../../state/ApiContext";
import { errorMessage } from "../../state/useAsync";
import { SectionDesc, SectionTitle } from "./rows";

const SHARE_TITLE = "Borealis logs";

export type ShareOutcome = "shared" | "copied" | "cancelled";

/**
 * Hand `text` to the system share sheet, else the clipboard. Web Share first (iOS); a web view
 * without it (Android) asks the backend's native share (`nativeShare`), and only if that fails too
 * the text goes to the clipboard. Resolves to what happened.
 */
export async function shareText(text: string, nativeShare?: () => Promise<void>): Promise<ShareOutcome> {
  const nav = navigator as Navigator & { share?: (d: { title?: string; text?: string }) => Promise<void> };
  if (typeof nav.share === "function") {
    try { await nav.share({ title: SHARE_TITLE, text }); return "shared"; }
    catch (e) { if ((e as { name?: string }).name === "AbortError") return "cancelled"; }
  } else if (nativeShare) {
    try { await nativeShare(); return "shared"; }
    catch { /* no native share sheet here: fall back to the clipboard */ }
  }
  await navigator.clipboard.writeText(text);
  return "copied";
}

/** Toast after the clipboard fallback: the share sheet did not open, so say where the logs went. */
export const COPIED_MESSAGE = "The share sheet could not open, so the logs were copied to the clipboard. Paste them into a message or email.";

/** DATA FILES > LOGS (UP3): desktop opens the logs folder; a phone has no folder to open, so it shares the newest lines. */
export function LogsRow({ mobile }: { mobile: boolean }) {
  const api = useApi();
  const toast = useToast();
  const openFolder = () => void api.openAppFolder({ folder: "logs" }).catch((e) => toast.push(errorMessage(e), "error"));
  const share = async () => {
    try {
      const text = await api.getRecentLogs();
      if (!text.trim()) { toast.push("There are no logs yet.", "info"); return; }
      const how = await shareText(text, () => api.shareLogs());
      if (how === "copied") toast.push(COPIED_MESSAGE, "info");
    } catch (e) { toast.push(errorMessage(e), "error"); }
  };
  return (
    <>
      <SectionTitle>LOGS</SectionTitle>
      <SectionDesc>Borealis keeps a few days of diagnostic logs on this device. They hold no character or source content and are never sent anywhere unless you choose to include them in a report.</SectionDesc>
      <div className="aur-set-left-clear">
        {mobile
          ? <Button onClick={() => void share()}>Share logs</Button>
          : <Button onClick={openFolder}>Open logs folder</Button>}
      </div>
    </>
  );
}
