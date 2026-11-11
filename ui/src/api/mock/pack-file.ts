/**
 * The web mock's `.borealis-pack.zip` (spec 14.2): a real zip (stored entries, CRC-32) so a pack exported from the mock
 * downloads as a file that imports again. The real backend writes deflated entries with the `zip` crate and reads both.
 */

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(bytes: Uint8Array): number {
  let c = 0xffffffff;
  for (const b of bytes) c = CRC_TABLE[(c ^ b) & 0xff]! ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

const LOCAL = 0x04034b50;
const CENTRAL = 0x02014b50;
const END = 0x06054b50;

/** A zip of `files` (name -> text), stored (no compression). */
export function zipFiles(files: Record<string, string>): Uint8Array {
  const enc = new TextEncoder();
  const locals: Uint8Array[] = [];
  const centrals: Uint8Array[] = [];
  let offset = 0;
  for (const [name, text] of Object.entries(files)) {
    const nameBytes = enc.encode(name);
    const body = enc.encode(text);
    const crc = crc32(body);
    const head = new DataView(new ArrayBuffer(30));
    head.setUint32(0, LOCAL, true); head.setUint16(4, 20, true); head.setUint16(8, 0, true);
    head.setUint32(14, crc, true); head.setUint32(18, body.length, true); head.setUint32(22, body.length, true); head.setUint16(26, nameBytes.length, true);
    const local = new Uint8Array([...new Uint8Array(head.buffer), ...nameBytes, ...body]);
    const cen = new DataView(new ArrayBuffer(46));
    cen.setUint32(0, CENTRAL, true); cen.setUint16(4, 20, true); cen.setUint16(6, 20, true);
    cen.setUint32(16, crc, true); cen.setUint32(20, body.length, true); cen.setUint32(24, body.length, true); cen.setUint16(28, nameBytes.length, true);
    cen.setUint32(42, offset, true);
    centrals.push(new Uint8Array([...new Uint8Array(cen.buffer), ...nameBytes]));
    locals.push(local);
    offset += local.length;
  }
  const dirSize = centrals.reduce((n, c) => n + c.length, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, END, true); end.setUint16(8, centrals.length, true); end.setUint16(10, centrals.length, true);
  end.setUint32(12, dirSize, true); end.setUint32(16, offset, true);
  const out = new Uint8Array(offset + dirSize + 22);
  let at = 0;
  for (const part of [...locals, ...centrals, new Uint8Array(end.buffer)]) { out.set(part, at); at += part.length; }
  return out;
}

/** The stored entries of a zip (name -> text); null when it is not a zip this mock can read. */
export function unzipFiles(bytes: Uint8Array): Record<string, string> | null {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let eocd = -1;
  for (let i = bytes.length - 22; i >= 0; i--) if (view.getUint32(i, true) === END) { eocd = i; break; }
  if (eocd < 0) return null;
  const count = view.getUint16(eocd + 10, true);
  let at = view.getUint32(eocd + 16, true);
  const dec = new TextDecoder();
  const out: Record<string, string> = {};
  for (let i = 0; i < count; i++) {
    if (view.getUint32(at, true) !== CENTRAL || view.getUint16(at + 10, true) !== 0) return null;
    const size = view.getUint32(at + 20, true);
    const nameLen = view.getUint16(at + 28, true);
    const extra = view.getUint16(at + 30, true) + view.getUint16(at + 32, true);
    const local = view.getUint32(at + 42, true);
    const name = dec.decode(bytes.subarray(at + 46, at + 46 + nameLen));
    const bodyAt = local + 30 + view.getUint16(local + 26, true) + view.getUint16(local + 28, true);
    out[name] = dec.decode(bytes.subarray(bodyAt, bodyAt + size));
    at += 46 + nameLen + extra;
  }
  return out;
}

export function toBase64(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

export function fromBase64(b64: string): Uint8Array {
  return Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
}
