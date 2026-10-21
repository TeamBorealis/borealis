import { useEffect, useSyncExternalStore } from "react";
import type { Api, AppUpdateStatus } from "../api/contract";
import { useApi } from "./ApiContext";

/** Latest release of the project; the same address the desktop backend asks. */
export const RELEASES_API = "https://api.github.com/repos/alon-mini/borealis/releases/latest";
/** The official download page (the website lists every platform). */
export const DOWNLOAD_PAGE = "https://borealisbuilder.com/#download";
const RELEASE_PAGE_PREFIX = "https://github.com/alon-mini/borealis/";
const DISMISSED_KEY = "borealis.updateDismissed";
const FETCH_TIMEOUT_MS = 10_000;
/** Phones wait this long after launch before asking, like the desktop backend. */
export const WEB_CHECK_DELAY_MS = 12_000;

/** `X.Y.Z` with an optional leading "v"; null for a pre-release or anything not numeric (same rules as the backend). */
export function parseVersion(raw: string): [number, number, number] | null {
  const s = raw.trim().replace(/^[vV]/, "").split("+")[0] ?? "";
  if (!s || s.includes("-")) return null;
  const parts = s.split(".");
  if (parts.length > 3) return null;
  const out: number[] = [];
  for (let i = 0; i < 3; i++) {
    const p = parts[i] ?? "0";
    if (!/^\d+$/.test(p)) return null;
    out.push(Number(p));
  }
  return out as [number, number, number];
}

export function isNewer(latest: string, current: string): boolean {
  const l = parseVersion(latest);
  const c = parseVersion(current);
  if (!l || !c) return false;
  for (let i = 0; i < 3; i++) if (l[i]! !== c[i]!) return l[i]! > c[i]!;
  return false;
}

const bare = (state: AppUpdateStatus["state"], current: string): AppUpdateStatus =>
  ({ state, current, latest: null, newer: false, url: null, releaseUrl: null, checkedAt: null });

/** Phones: one GET to GitHub with `fetch()` (no cookies, no custom headers), same answer shape as the backend. Never throws. */
export async function checkWithFetch(current: string, fetchFn: typeof fetch = fetch): Promise<AppUpdateStatus> {
  const now = new Date().toISOString();
  try {
    const ctl = new AbortController();
    const timer = setTimeout(() => ctl.abort(), FETCH_TIMEOUT_MS);
    const res = await fetchFn(RELEASES_API, { credentials: "omit", signal: ctl.signal }).finally(() => clearTimeout(timer));
    if (!res.ok) return { ...bare("failed", current), checkedAt: now };
    const rel = (await res.json()) as { tag_name?: unknown; html_url?: unknown; draft?: unknown; prerelease?: unknown };
    const v = typeof rel.tag_name === "string" && !rel.draft && !rel.prerelease ? parseVersion(rel.tag_name) : null;
    if (!v) return { ...bare("failed", current), checkedAt: now };
    const html = typeof rel.html_url === "string" && rel.html_url.startsWith(RELEASE_PAGE_PREFIX) ? rel.html_url : null;
    const latest = v.join(".");
    return { state: "done", current, latest, newer: isNewer(latest, current), url: DOWNLOAD_PAGE, releaseUrl: html, checkedAt: now };
  } catch {
    return { ...bare("failed", current), checkedAt: now };
  }
}

/* ---- shared store: the title-bar update button and Settings read the same answer ---- */
let status: AppUpdateStatus | null = null;
let dismissed = readDismissed();
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

function readDismissed(): string | null {
  try { return localStorage.getItem(DISMISSED_KEY); } catch { return null; }
}

export function setAppUpdate(next: AppUpdateStatus): void { status = next; emit(); }
export const getAppUpdate = (): AppUpdateStatus | null => status;
/** Test hook: forget the stored answer and the remembered dismissal. */
export function resetAppUpdate(): void { status = null; try { localStorage.removeItem(DISMISSED_KEY); } catch { /* storage unavailable */ } dismissed = null; emit(); }

/** Hide the update button for this version; remembered across launches. */
export function dismissAppUpdate(version: string): void {
  dismissed = version;
  try { localStorage.setItem(DISMISSED_KEY, version); } catch { /* storage unavailable: hidden until restart */ }
  emit();
}

export interface AppUpdateView { status: AppUpdateStatus | null; showNotice: boolean }

let cached: AppUpdateView = { status: null, showNotice: false };
function snapshot(): AppUpdateView {
  const show = !!status && status.state === "done" && status.newer && !!status.latest && status.latest !== dismissed;
  if (cached.status !== status || cached.showNotice !== show) cached = { status, showNotice: show };
  return cached;
}

export function useAppUpdateView(): AppUpdateView {
  return useSyncExternalStore((cb) => { listeners.add(cb); return () => { listeners.delete(cb); }; }, snapshot, snapshot);
}

/** "Check now" (and the startup check on phones): the backend answers on desktop, `fetch()` on phones. */
export async function checkAppUpdateNow(api: Api): Promise<AppUpdateStatus> {
  let s = await api.checkAppUpdate();
  if (s.state === "unsupported") s = await checkWithFetch(await api.appVersion());
  setAppUpdate(s);
  return s;
}

/** Open the download page: the backend opens the system browser (desktop and phones); `window.open` is only a fallback. */
export async function openDownload(api: Api, url: string = DOWNLOAD_PAGE): Promise<void> {
  try { await api.openDownloadPage(); } catch { window.open(url, "_blank", "noopener,noreferrer"); }
}

/** App-wide watcher (mounted once in the shell): reads the startup result, follows the backend event, and on phones runs the delayed check itself. */
export function useAppUpdateWatcher(): void {
  const api = useApi();
  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    void (async () => {
      let s: AppUpdateStatus;
      try { s = await api.appUpdateStatus(); } catch { return; }
      if (!alive) return;
      if (s.state !== "unsupported") { if (s.state !== "idle") setAppUpdate(s); return; }
      timer = setTimeout(() => {
        void (async () => {
          try {
            if (!alive || !(await api.getSettings()).checkAppUpdates) return;
            const web = await checkWithFetch(await api.appVersion());
            if (alive) setAppUpdate(web);
          } catch { /* silent */ }
        })();
      }, WEB_CHECK_DELAY_MS);
    })();
    const off = api.subscribe("appupdate:status", (p) => { if (p.state !== "idle") setAppUpdate(p); });
    return () => { alive = false; clearTimeout(timer); off(); };
  }, [api]);
}
