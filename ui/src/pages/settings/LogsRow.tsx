import { Button, useToast } from "../../components";
import { useApi } from "../../state/ApiContext";
import { errorMessage } from "../../state/useAsync";
import { FieldLabel, Help } from "./rows";

const SHARE_TITLE = "Borealis logs";

/** Hand `text` to the system share sheet, else the clipboard. Resolves to what happened. */
export async function shareText(text: string): Promise<"shared" | "copied" | "cancelled"> {
  const nav = navigator as Navigator & { share?: (d: { title?: string; text?: string }) => Promise<void> };
  if (typeof nav.share === "function") {
    try { await nav.share({ title: SHARE_TITLE, text }); return "shared"; }
    catch (e) { if ((e as { name?: string }).name === "AbortError") return "cancelled"; }
  }
  await navigator.clipboard.writeText(text);
  return "copied";
}

/** DATA FILES > LOGS (UP3): desktop opens the logs folder; a phone has no folder to open, so it shares the newest lines. */
export function LogsRow({ mobile }: { mobile: boolean }) {
  const api = useApi();
  const toast = useToast();
  const openFolder = () => void api.openAppFolder({ folder: "logs" }).catch((e) => toast.push(errorMessage(e), "error"));
  const share = async () => {
    try {
      const text = await api.getRecentLogs();
      if (!text.trim()) { toast.push("There are no logs yet.", "info"); return; }
      const how = await shareText(text);
      if (how === "copied") toast.push("Logs copied to the clipboard.", "info");
    } catch (e) { toast.push(errorMessage(e), "error"); }
  };
  return (
    <>
      <FieldLabel>LOGS</FieldLabel>
      <Help>Borealis keeps a few days of diagnostic logs on this device. They hold no character or source content, and Borealis never sends them anywhere.</Help>
      <div className="aur-set-left-clear">
        {mobile
          ? <Button onClick={() => void share()}>Share logs</Button>
          : <Button onClick={openFolder}>Open logs folder</Button>}
      </div>
    </>
  );
}
