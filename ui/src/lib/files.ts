export const readBuffer = (f: Blob): Promise<ArrayBuffer> =>
  new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result as ArrayBuffer);
    r.onerror = () => reject(r.error ?? new Error("read failed"));
    r.readAsArrayBuffer(f);
  });

/** Reads a File as base64 (no data: prefix). */
export async function fileToBase64(file: File): Promise<string> {
  const buf = new Uint8Array(await readBuffer(file));
  let bin = "";
  for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
  return btoa(bin);
}

export async function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(r.error);
    r.readAsDataURL(file);
  });
}

/** Saves base64 bytes through the browser (used when the backend returns no path). */
export function downloadBase64(fileName: string, mime: string, b64: string): void {
  const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
  const a = document.createElement("a");
  a.href = url;
  a.download = fileName;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** Files as the import commands take them. */
export async function filesToInputs(files: readonly File[]): Promise<{ fileName: string; contentBase64: string }[]> {
  return Promise.all(files.map(async (f) => ({ fileName: f.name, contentBase64: await fileToBase64(f) })));
}

/** A File from base64 bytes (files the OS opened with the app arrive this way). */
export function base64ToFile(fileName: string, b64: string): File {
  return new File([Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))], fileName);
}
