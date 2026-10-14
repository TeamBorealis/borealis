import { IMPORT_LIMITS, type Api, type ImportSource } from "../../api/contract";

/** Bytes to base64 without building one huge string through String.fromCharCode(...). */
export function bytesToBase64(bytes: Uint8Array): string {
  let bin = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) bin += String.fromCharCode(...bytes.subarray(i, i + chunk));
  return btoa(bin);
}

const DOCUMENT_TYPES = ["application/vnd.openxmlformats-officedocument.wordprocessingml.document", "application/vnd.oasis.opendocument.text"];

const kindOf = (f: { name: string; type: string }): "image" | "pdf" | "document" | "text" | null => {
  const n = f.name.toLowerCase();
  if (f.type.startsWith("image/") || /\.(png|jpe?g|webp)$/.test(n)) return "image";
  if (f.type === "application/pdf" || n.endsWith(".pdf")) return "pdf";
  if (DOCUMENT_TYPES.includes(f.type) || /\.(docx|odt)$/.test(n)) return "document";
  if (f.type.startsWith("text/") || f.type === "application/json" || /\.(txt|md|markdown|json)$/.test(n)) return "text";
  return null;
};

const mb = (b: number) => `${Math.round(b / (1024 * 1024))} MB`;

/** A friendly reason a file cannot be imported, or null. Mirrors the limits Rust enforces. */
export function problemWith(f: { name: string; type: string; size: number }): string | null {
  const kind = kindOf(f);
  if (!kind) return `"${f.name}" is not supported. Use an image (PNG, JPEG, WebP), a PDF, a Word (.docx) or OpenDocument (.odt) file, or a text file.`;
  if (f.size === 0) return `"${f.name}" is empty.`;
  const limit = kind === "image" ? IMPORT_LIMITS.maxImageBytes : kind === "pdf" ? IMPORT_LIMITS.maxPdfBytes : kind === "document" ? IMPORT_LIMITS.maxDocumentBytes : IMPORT_LIMITS.maxTextChars * 4;
  if (f.size > limit) return `"${f.name}" is too large (the limit is ${mb(limit)}).`;
  return null;
}

const readBuffer = (f: Blob): Promise<ArrayBuffer> =>
  new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as ArrayBuffer);
    reader.onerror = () => reject(reader.error ?? new Error("read failed"));
    reader.readAsArrayBuffer(f);
  });

/** Read the contents the webview has (drop, picker, camera) into `bytes` sources. */
export async function fileToSource(f: File): Promise<ImportSource> {
  const base64 = bytesToBase64(new Uint8Array(await readBuffer(f)));
  return { kind: "bytes", name: f.name, mime: f.type || "application/octet-stream", base64 };
}

/** True inside the Tauri shell, where the native file dialog exists. */
export const hasNativeDialog = (): boolean => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** A file chosen in the backend's native dialog: the import sends only the opaque token. */
export interface NativePick { name: string; source: Extract<ImportSource, { kind: "file" }> }

/** Native "Browse..." dialog (multi-select), opened by the backend (`pickPath`); [] when cancelled. */
export async function browseNative(api: Api): Promise<NativePick[]> {
  const picks = await api.pickPath({ purpose: "import-file" });
  return picks.map((p) => ({ name: p.name, source: { kind: "file", token: p.token } }));
}
