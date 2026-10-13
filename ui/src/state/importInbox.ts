/** Files dropped anywhere in the app are parked here until the Import page mounts. */
let pending: File[] = [];

export function stashImportFiles(files: File[]): void {
  pending = [...pending, ...files];
}

export function takeImportFiles(): File[] {
  const out = pending;
  pending = [];
  return out;
}
