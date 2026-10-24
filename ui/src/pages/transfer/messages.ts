import type { LinkPhase, LinkPlatform, LinkPreviewAction, LinkStatus, LinkSummary } from "../../api/contract";

/** Phases after which a session does nothing more. */
export const FINAL_PHASES: readonly LinkPhase[] = ["done", "failed", "cancelled", "declined", "expired"];
export const isFinal = (p: LinkPhase): boolean => FINAL_PHASES.includes(p);

const RANK: Record<LinkPhase, number> = {
  waiting: 0, connecting: 1, preview: 2, waiting_for_peer: 2, transferring: 3, applying: 4,
  done: 5, failed: 5, cancelled: 5, declined: 5, expired: 5,
};
/** True when `next` may replace `cur` (a slow command reply never undoes a newer event). */
export function isNewer(cur: LinkStatus | null, next: LinkStatus): boolean {
  if (!cur || cur.sessionId !== next.sessionId) return true;
  return !isFinal(cur.phase) && RANK[next.phase] >= RANK[cur.phase];
}

/** Preview badge per planner action (newest edit wins, SY4). */
export const ACTION_LABEL: Record<LinkPreviewAction, string> = {
  new: "New",
  replace: "Replaces your older copy",
  keep_local: "Yours is newer: kept",
  identical: "Already here",
};

/** "1 new, 1 replaced, 1 kept (newer there)" from a summary; `here` = this device received. */
export function summaryText(s: LinkSummary, here: boolean): string {
  const parts = [
    s.added && `${s.added} new`,
    s.replaced && `${s.replaced} replaced`,
    s.keptLocal && `${s.keptLocal} kept (${here ? "yours was newer" : "theirs was newer"})`,
    s.identical && `${s.identical} already there`,
  ].filter(Boolean);
  return parts.length ? parts.join(", ") : "nothing changed";
}

const peer = (s: LinkStatus): string => s.peerName ?? "the other device";

/** The status-bar line for a finished session (classic builder status style: one sentence, no toast). */
export function statusLine(s: LinkStatus): string | null {
  switch (s.phase) {
    case "done":
      return s.role === "receive"
        ? `Received from ${peer(s)}: ${summaryText(s.summary ?? { added: 0, replaced: 0, keptLocal: 0, identical: 0 }, true)}.`
        : `Sent to ${peer(s)}: ${summaryText(s.summary ?? { added: 0, replaced: 0, keptLocal: 0, identical: 0 }, false)}.`;
    case "declined":
      return s.role === "receive" ? "Transfer declined. Nothing was written." : `${peer(s)} declined the transfer.`;
    case "cancelled":
      return "Transfer cancelled. Nothing was changed.";
    case "expired":
      return "The code expired before another device connected.";
    case "failed":
      return `Transfer failed: ${failureText(s)}`;
    default:
      return null;
  }
}

/** What went wrong, in words, for the dialog and the status bar. */
export function failureText(s: LinkStatus): string {
  switch (s.errorCode) {
    case "wrong_code": return "the code did not match. It works only once: show a new code on the other device and try again.";
    case "expired": return "the code expired. Show a new code on the other device.";
    case "pin_mismatch": return "the other device did not match the scanned code. Scan it again.";
    case "not_found": return "no device with that code was found. Check that both are on the same Wi-Fi, or type the address too.";
    case "connection": return "could not reach the other device. Check that both are on the same Wi-Fi and the window is still open.";
    case "peer_cancelled": return "the other device cancelled.";
    case "refused": return s.error ?? "the other device refused the transfer.";
    default: return s.error ?? "something went wrong.";
  }
}

/** "48 KB" style sizes. */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

/** "4:59" from seconds. */
export function formatCountdown(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

export const PLATFORM_LABEL: Record<LinkPlatform, string> = {
  macos: "Mac", windows: "Windows PC", linux: "Linux PC", ios: "iPhone or iPad", android: "Android device", unknown: "Device",
};

/** "24 Oct 2026" from unix seconds. */
export function formatDay(unixSecs: number): string {
  return new Date(unixSecs * 1000).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
}

const plural = (n: number, one: string, many: string): string => `${n} ${n === 1 ? one : many}`;

/** Status-bar line after "Open bundle": what the file changed here. */
export function bundleOpenedLine(fileName: string, s: LinkSummary): string {
  return `Opened ${fileName}: ${summaryText(s, true)}.`;
}

/** Status-bar line after "Export to bundle" wrote a file. */
export function bundleExportedLine(fileName: string, characters: number, packs: number): string {
  const packText = packs > 0 ? ` and ${plural(packs, "content pack", "content packs")}` : "";
  return `Exported ${plural(characters, "character", "characters")}${packText} to ${fileName}.`;
}

/** Status-bar line after the share sheet took the bundle. */
export function bundleSharedLine(fileName: string): string {
  return `Shared ${fileName}. Open it with Borealis on the other device.`;
}
