/** What `shareBase64File` did. */
export type WebShareResult = "shared" | "cancelled" | "unsupported";

type ShareNavigator = Navigator & {
  canShare?: (d: { files: File[] }) => boolean;
  share?: (d: { files: File[] }) => Promise<void>;
};

/** A File from base64 bytes. */
export function base64File(fileName: string, mime: string, b64: string): File {
  return new File([Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))], fileName, { type: mime });
}

/**
 * Hands a file to the web view's own share sheet (iOS). Call it straight from the tap, with the bytes already in hand:
 * the share sheet needs the tap's user activation, which an awaited backend call can use up.
 *
 * Only the file is shared, with no `title` or `text`: WebKit on iOS adds either one as a second, text item, so Save to
 * Files saved a stray `text.txt` beside the file. The file's name is what the share sheet shows.
 */
export async function shareBase64File(fileName: string, mime: string, b64: string): Promise<WebShareResult> {
  const nav = navigator as ShareNavigator;
  const file = base64File(fileName, mime, b64);
  if (typeof nav.share !== "function" || !nav.canShare?.({ files: [file] })) return "unsupported";
  try {
    await nav.share({ files: [file] });
    return "shared";
  } catch (e) {
    if ((e as { name?: string }).name === "AbortError") return "cancelled";
    throw e;
  }
}
