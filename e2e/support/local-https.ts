import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createServer } from 'node:https';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { MobilePlatform } from './mobile.js';

/**
 * A local HTTPS server with a self-signed certificate, so a phone test can hit a host whose
 * certificate nobody signed without depending on the internet (B-42). The key and certificate are
 * made at test time in a temp folder (openssl) and deleted at once; nothing secret is in the repo.
 */

/** How the device reaches the Mac: the Android emulator's alias for the host loopback, and the
 * iOS simulator, which shares the host network. */
const DEVICE_HOST: Record<MobilePlatform, string> = { android: '10.0.2.2', ios: '127.0.0.1' };

export function deviceHostAddress(platform: MobilePlatform): string {
  return DEVICE_HOST[platform];
}

const CERT_DAYS = '2';
const ALL_INTERFACES = '0.0.0.0';

export interface LocalHttpsServer {
  /** Base URL as the device sees it, e.g. https://10.0.2.2:53122 */
  readonly baseUrl: string;
  /** TLS handshakes the client gave up on so far (proof the device reached this server). */
  refusedHandshakes(): number;
  close(): Promise<void>;
}

/** Self-signed key + certificate for `host` (valid for it, so the only fault is the issuer). */
function selfSignedPair(host: string): { key: Buffer; cert: Buffer } {
  const dir = mkdtempSync(join(tmpdir(), 'b42-cert-'));
  try {
    const key = join(dir, 'key.pem');
    const cert = join(dir, 'cert.pem');
    const names = [...new Set([`IP:${host}`, 'IP:127.0.0.1'])].concat('DNS:localhost').join(',');
    execFileSync('openssl', [
      'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-sha256', '-days', CERT_DAYS,
      '-keyout', key, '-out', cert, '-subj', `/CN=${host}`,
      '-addext', `subjectAltName=${names}`,
    ], { stdio: ['ignore', 'ignore', 'pipe'] });
    return { key: readFileSync(key), cert: readFileSync(cert) };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/**
 * Serve `files` (path -> body) over HTTPS on a free port, on all interfaces. Other paths answer 404.
 * Call `close()` when the test is done.
 */
export async function startSelfSignedServer(platform: MobilePlatform, files: Readonly<Record<string, string>>): Promise<LocalHttpsServer> {
  const host = deviceHostAddress(platform);
  const requests: string[] = [];
  let refused = 0;
  const server = createServer(selfSignedPair(host), (req, res) => {
    requests.push(req.url ?? '');
    const body = files[req.url ?? ''];
    res.writeHead(body === undefined ? 404 : 200, { 'content-type': 'text/plain' });
    res.end(body ?? 'not found');
  });
  // A handshake the client rejects (the point of the test) surfaces here; it is expected, not a failure.
  server.on('tlsClientError', () => { refused += 1; });
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, ALL_INTERFACES, () => resolve());
  });
  const { port } = server.address() as AddressInfo;
  return {
    baseUrl: `https://${host}:${port}`,
    refusedHandshakes: () => refused,
    close: () => new Promise<void>((resolve) => {
      console.info(`[local-https] ${refused} handshake(s) refused by the client; served: ${requests.join(', ') || 'nothing'}`);
      server.closeAllConnections();
      server.close(() => resolve());
    }),
  };
}
