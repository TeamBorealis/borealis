import type { Api } from "../../api/contract";

/** A `.borealis` file waiting to be previewed: a `bundle-file` token and the name to show. */
export interface BundleRequest {
  token: string;
  name: string;
}

const OPEN_EVENT = "borealis:open-bundle";
/** Fired after a bundle was applied, so lists of characters refresh. */
const APPLIED_EVENT = "borealis:bundle-applied";

/** Preview a `.borealis` file (a drop, the OS "open with", or File > Open bundle...). */
export function openBundleFile(request: BundleRequest): void {
  window.dispatchEvent(new CustomEvent<BundleRequest>(OPEN_EVENT, { detail: request }));
}

/** Subscribe to open requests (the shell's BundleHost). */
export function onBundleRequest(handler: (r: BundleRequest) => void): () => void {
  const on = (e: Event) => handler((e as CustomEvent<BundleRequest>).detail);
  window.addEventListener(OPEN_EVENT, on);
  return () => window.removeEventListener(OPEN_EVENT, on);
}

export function announceBundleApplied(): void {
  window.dispatchEvent(new Event(APPLIED_EVENT));
}

/** Subscribe to "a bundle was applied" (lists of characters re-read). */
export function onBundleApplied(handler: () => void): () => void {
  window.addEventListener(APPLIED_EVENT, handler);
  return () => window.removeEventListener(APPLIED_EVENT, handler);
}

/** File > Open bundle... : the backend's open panel (desktop), then the preview. A cancelled panel does nothing. */
export async function pickAndOpenBundle(api: Api): Promise<void> {
  const [picked] = await api.pickPath({ purpose: "bundle-file" });
  if (picked) openBundleFile({ token: picked.token, name: picked.name });
}

/** Files the OS opened with the app, set aside as bundles by the backend (call after `takeOpenedFiles`). */
export async function openOsBundles(api: Api): Promise<void> {
  const bundles = await api.takeOpenedBundles().catch(() => []);
  bundles.forEach((b) => openBundleFile({ token: b.token, name: b.name }));
}
