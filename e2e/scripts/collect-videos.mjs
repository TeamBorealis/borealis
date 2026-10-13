#!/usr/bin/env node
// Collect the tour recordings from a tour run into <repo>/videos/<YYYY-MM-DD>/ as H.264 MP4 (QuickTime-playable),
// plus web-reel.mp4 (all tours back to back) and index.html (a <video controls> per tour with its caption chapters).
//   node scripts/collect-videos.mjs [--results .e2e/tour-web/results] [--out <dir>]
// Also collects the iOS-simulator and Android-emulator tours (.e2e/tour-ios, .e2e/tour-android: report.json + the MP4s the
// run recorded plus the caption logs in .e2e/tour/<platform>-<tour>.json) into ios-*.mp4 / android-*.mp4, burns the captions in
// on a strip under the device picture (ffmpeg here has no drawtext; the strips are PNGs drawn with python3 + Pillow), and writes
// mobile-reel.mp4. index.html then has three sections: Web (mock backend), iOS simulator, Android emulator.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const FFMPEG = process.env.FFMPEG ?? '/opt/homebrew/bin/ffmpeg';
const FFPROBE = process.env.FFPROBE ?? '/opt/homebrew/bin/ffprobe';
const here = dirname(fileURLToPath(import.meta.url));
const e2eRoot = resolve(here, '..');
const arg = (name, dflt) => { const i = process.argv.indexOf(name); return i > 0 ? process.argv[i + 1] : dflt; };
const webResults = [arg('--results', '.e2e/tour-web/results'), '.e2e/tour-run/results'].map((r) => resolve(e2eRoot, r));
const resultsDir = webResults.find((r) => existsSync(r)) ?? webResults[0];
const captionDir = resolve(e2eRoot, '.e2e/tour');
const today = new Date();
const date = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(today.getDate()).padStart(2, '0')}`;
const outDir = resolve(arg('--out', join(e2eRoot, '..', '..', 'videos', date)));
const TOURS = ['tour-1-create', 'tour-2-levelup', 'tour-3-manage'];
const CAPTION_LEAD_S = 0; // report.json: the video segment starts within ~1 ms of the tour clock

const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const mmss = (s) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`;
const run = (bin, args) => execFileSync(bin, args, { stdio: ['ignore', 'pipe', 'pipe'] }).toString();
const duration = (f) => Number(run(FFPROBE, ['-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', f]).trim());
const sizeMb = (f) => (statSync(f).size / 1048576).toFixed(1);

function findVideos(dir, prefix) {
  if (!existsSync(dir)) return [];
  const test = readdirSync(dir).find((d) => d.startsWith(`${prefix}-`));
  if (!test) return [];
  const found = [];
  const walk = (d) => { for (const e of readdirSync(d, { withFileTypes: true })) { const p = join(d, e.name); if (e.isDirectory()) walk(p); else if (/^video(-part\d+)?\.webm$/.test(e.name)) found.push(p); } };
  walk(join(dir, test));
  return found.sort();
}

mkdirSync(outDir, { recursive: true });

// ---------------------------------------------------------------- web (mock backend): unchanged behaviour
const web = [];
for (const name of TOURS) {
  const webms = findVideos(resultsDir, name);
  const mp4 = join(outDir, `web-${name}.mp4`);
  if (webms.length) {
    // Even dimensions for yuv420p (1355x847 gets one padded row/column), H.264 crf 23, faststart for QuickTime/web.
    run(FFMPEG, ['-y', '-loglevel', 'error', '-i', webms[0], '-vf', 'pad=ceil(iw/2)*2:ceil(ih/2)*2:0:0:black,fps=25', '-c:v', 'libx264', '-crf', '23', '-preset', 'medium',
      '-pix_fmt', 'yuv420p', '-movflags', '+faststart', '-an', mp4]);
  } else if (!existsSync(mp4)) { console.warn(`no web video for ${name} under ${resultsDir}`); continue; }
  const meta = existsSync(join(captionDir, `${name}.json`)) ? JSON.parse(readFileSync(join(captionDir, `${name}.json`), 'utf8')) : { title: name, captions: [], problems: [] };
  const chapters = meta.captions.map((c) => ({ text: c.text, at: Math.max(0, c.t / 1000 + CAPTION_LEAD_S) }));
  web.push({ name, file: `web-${name}.mp4`, mp4, title: meta.title ?? name, chapters, problems: meta.problems ?? [], seconds: duration(mp4) });
  console.log(`${name}: ${mmss(web.at(-1).seconds)} ${sizeMb(mp4)} MB`);
}

// Reel: all tours back to back (identical encode settings, so the concat demuxer can stream-copy).
function webReel() {
  const list = join(outDir, '.reel.txt');
  writeFileSync(list, web.map((d) => `file '${d.mp4.replace(/'/g, "'\\''")}'`).join('\n'));
  const reel = join(outDir, 'web-reel.mp4');
  run(FFMPEG, ['-y', '-loglevel', 'error', '-f', 'concat', '-safe', '0', '-i', list, '-c', 'copy', '-movflags', '+faststart', reel]);
  unlinkSync(list);
  console.log(`web-reel: ${mmss(duration(reel))} ${sizeMb(reel)} MB`);
}
if (web.length) webReel();

// ---------------------------------------------------------------- mobile (real app on a simulator / emulator)
const MOBILE = [
  { platform: 'ios', label: 'iOS simulator (real app)', dir: '.e2e/tour-ios' },
  { platform: 'android', label: 'Android emulator (real app)', dir: '.e2e/tour-android' },
];
const DEVICE_H = 1280; // device picture height after scaling; the width keeps the device aspect ratio (even)
const STRIP_H = 168;   // caption strip under the picture
const REEL_W = 720;

/** Draw one transparent caption strip PNG per text (width x STRIP_H) with python3 + Pillow; the texts go in on stdin. */
function renderCaptionPngs(texts, width, dir) {
  const py = `
import json, sys
from PIL import Image, ImageDraw, ImageFont
texts, width, height, out = json.load(sys.stdin), int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
FONT = '/System/Library/Fonts/Helvetica.ttc'
def wrap(draw, text, font, maxw):
    lines, cur = [], ''
    for w in text.split():
        t = (cur + ' ' + w).strip()
        if draw.textlength(t, font=font) <= maxw or not cur: cur = t
        else: lines.append(cur); cur = w
    return lines + [cur]
for i, text in enumerate(texts):
    img = Image.new('RGBA', (width, height), (14, 16, 20, 255))
    d = ImageDraw.Draw(img)
    size = 40
    while True:
        font = ImageFont.truetype(FONT, size)
        lines = wrap(d, text, font, width - 48)
        if len(lines) <= 2 and size >= 20 or size <= 20: break
        size -= 2
    lh = size * 1.25
    y = (height - lh * len(lines)) / 2
    for ln in lines:
        d.text(((width - d.textlength(ln, font=font)) / 2, y), ln, font=font, fill=(255, 255, 255, 255))
        y += lh
    img.save(f'{out}/cap{i}.png')
`;
  execFileSync('python3', ['-E', '-c', py, String(width), String(STRIP_H), dir], { input: JSON.stringify(texts), stdio: ['pipe', 'pipe', 'pipe'] });
}

/** Re-encode a device MP4 (H.264, device aspect kept) with a caption strip underneath and the captions burned onto it. */
function encodeMobile(src, dst, captions, videoStartMs) {
  const probe = run(FFPROBE, ['-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=width,height', '-of', 'csv=p=0', src]).trim().split(',').map(Number);
  const w = 2 * Math.round((probe[0] * DEVICE_H / probe[1]) / 2);
  const total = duration(src);
  const chapters = captions.map((c) => ({ text: c.text, at: Math.max(0, (c.at - videoStartMs) / 1000) })).filter((c) => c.at < total);
  const tmp = mkdtempSync(join(tmpdir(), 'borealis-caps-'));
  try {
    renderCaptionPngs(chapters.map((c) => c.text), w, tmp);
    const args = ['-y', '-loglevel', 'error', '-i', src];
    chapters.forEach((_, i) => args.push('-i', join(tmp, `cap${i}.png`)));
    let graph = `[0:v]scale=${w}:${DEVICE_H},fps=30,pad=${w}:${DEVICE_H + STRIP_H}:0:0:black[b0]`;
    chapters.forEach((c, i) => {
      const end = i + 1 < chapters.length ? chapters[i + 1].at : total + 1;
      graph += `;[b${i}][${i + 1}:v]overlay=0:${DEVICE_H}:enable='between(t,${c.at.toFixed(2)},${end.toFixed(2)})'[b${i + 1}]`;
    });
    args.push('-filter_complex', graph, '-map', `[b${chapters.length}]`, '-t', total.toFixed(2), '-c:v', 'libx264', '-crf', '23', '-preset', 'medium', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', '-an', dst);
    run(FFMPEG, args);
  } finally { rmSync(tmp, { recursive: true, force: true }); }
  return chapters;
}

const mobile = MOBILE.map(({ platform, label, dir }) => ({ platform, label, items: [] }));
for (const [mi, { platform, dir }] of MOBILE.entries()) {
  const out = resolve(e2eRoot, dir);
  const reportFile = join(out, 'report.json');
  if (!existsSync(reportFile)) { console.warn(`no ${platform} run at ${out}`); continue; }
  const report = JSON.parse(readFileSync(reportFile, 'utf8'));
  for (const r of report.run?.results ?? []) {
    const art = r.attempts?.at(-1)?.artifacts?.find((a) => a.kind === 'video');
    const m = /^(tour-m[\w-]+)-(ios|android)$/.exec(r.titlePath?.[0] ?? '');
    if (!r.selected || r.status !== 'passed' || !art || !m || m[2] !== platform) continue;
    const name = m[1];
    const logFile = join(captionDir, `${platform}-${name}.json`);
    if (!existsSync(logFile)) { console.warn(`no caption log for ${platform} ${name}`); continue; }
    const log = JSON.parse(readFileSync(logFile, 'utf8'));
    const file = `${platform}-${name}.mp4`;
    const mp4 = join(outDir, file);
    const chapters = encodeMobile(join(out, 'results', art.path), mp4, log.captions, Date.parse(art.startedAt));
    mobile[mi].items.push({ name, file, mp4, title: log.title ?? name, chapters, problems: [], seconds: duration(mp4) });
    console.log(`${platform} ${name}: ${mmss(mobile[mi].items.at(-1).seconds)} ${sizeMb(mp4)} MB`);
  }
  mobile[mi].items.sort((a, b) => a.name.localeCompare(b.name));
}

// mobile-reel.mp4: every mobile tour on one 720x1440 canvas (device picture centred, no stretching), iOS tours first.
const mobileAll = mobile.flatMap((s) => s.items);
if (mobileAll.length) {
  const reel = join(outDir, 'mobile-reel.mp4');
  const args = ['-y', '-loglevel', 'error'];
  mobileAll.forEach((d) => args.push('-i', d.mp4));
  const H = DEVICE_H + STRIP_H;
  const graph = mobileAll.map((_, i) => `[${i}:v]scale=${REEL_W}:${H}:force_original_aspect_ratio=decrease,pad=${REEL_W}:${H}:(ow-iw)/2:0:black,setsar=1,fps=30[r${i}]`).join(';')
    + `;${mobileAll.map((_, i) => `[r${i}]`).join('')}concat=n=${mobileAll.length}:v=1:a=0[o]`;
  args.push('-filter_complex', graph, '-map', '[o]', '-c:v', 'libx264', '-crf', '23', '-preset', 'medium', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', '-an', reel);
  run(FFMPEG, args);
  console.log(`mobile-reel: ${mmss(duration(reel))} ${sizeMb(reel)} MB`);
}

// ---------------------------------------------------------------- macOS (real Borealis.app; whole-screen recording cropped to the window)
const macosItems = [];
{
  const out = resolve(e2eRoot, '.e2e/tour-macos');
  const reportFile = join(out, 'report.json');
  const report = existsSync(reportFile) ? JSON.parse(readFileSync(reportFile, 'utf8')) : { run: { results: [] } };
  for (const r of report.run?.results ?? []) {
    const art = r.attempts?.at(-1)?.artifacts?.find((a) => a.kind === 'video');
    const m = /^tour-(macos-[\w-]+?)-macos$/.exec(r.titlePath?.[0] ?? '');
    if (!r.selected || r.status !== 'passed' || !art || !m) continue;
    const logFile = join(captionDir, `macos-tour-${m[1]}.json`);
    if (!existsSync(logFile)) { console.warn(`no caption log for macos ${m[1]}`); continue; }
    const log = JSON.parse(readFileSync(logFile, 'utf8'));
    const src = join(out, 'results', art.path);
    const win = JSON.parse(readFileSync(join(dirname(src), 'window.json'), 'utf8'));
    const file = 'macos-tour.mp4';
    const mp4 = join(outDir, file);
    const chapters = encodeMacos(src, mp4, log.captions, Date.parse(art.startedAt), win);
    macosItems.push({ name: m[1], file, mp4, title: log.title ?? m[1], chapters, problems: [], seconds: duration(mp4) });
    console.log(`macos ${m[1]}: ${mmss(macosItems.at(-1).seconds)} ${sizeMb(mp4)} MB`);
  }
}

/** Crop the whole-screen recording to the app window (window.json is in screen points; the video is in pixels), add a caption strip. */
function encodeMacos(src, dst, captions, videoStartMs, win) {
  const probe = run(FFPROBE, ['-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=width,height', '-of', 'csv=p=0', src]).trim().split(',').map(Number);
  const k = probe[0] / win.screenWidth;
  const even = (n) => 2 * Math.round(n / 2);
  const cw = even(win.width * k), ch = even(win.height * k), cx = Math.round(win.x * k), cy = Math.round(win.y * k);
  const w = 1200, h = Math.round(win.height * 1200 / win.width / 2) * 2;
  const total = duration(src);
  const chapters = captions.map((c) => ({ text: c.text, at: Math.max(0, (c.at - videoStartMs) / 1000) })).filter((c) => c.at < total);
  const tmp = mkdtempSync(join(tmpdir(), 'borealis-caps-'));
  try {
    renderCaptionPngs(chapters.map((c) => c.text), w, tmp);
    const args = ['-y', '-loglevel', 'error', '-i', src];
    chapters.forEach((_, i) => args.push('-i', join(tmp, `cap${i}.png`)));
    let graph = `[0:v]crop=${cw}:${ch}:${cx}:${cy},scale=${w}:${h},fps=30,pad=${w}:${h + STRIP_H}:0:0:black[b0]`;
    chapters.forEach((c, i) => {
      const end = i + 1 < chapters.length ? chapters[i + 1].at : total + 1;
      graph += `;[b${i}][${i + 1}:v]overlay=0:${h}:enable='between(t,${c.at.toFixed(2)},${end.toFixed(2)})'[b${i + 1}]`;
    });
    args.push('-filter_complex', graph, '-map', `[b${chapters.length}]`, '-t', total.toFixed(2), '-c:v', 'libx264', '-crf', '23', '-preset', 'medium', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', '-an', dst);
    run(FFMPEG, args);
  } finally { rmSync(tmp, { recursive: true, force: true }); }
  return chapters;
}
if (!web.length && !mobileAll.length && !macosItems.length) { console.error('nothing to collect'); process.exit(1); }

// ---------------------------------------------------------------- index.html
const clip = (d, i, kind) => {
  const items = d.chapters.map((c) => `<li><a href="#" data-seek="${c.at.toFixed(1)}">${mmss(c.at)}</a> ${esc(c.text)}</li>`).join('\n');
  const problems = d.problems.length ? `<p class="warn">Steps that failed during recording: ${esc(d.problems.join('; '))}</p>` : '';
  const id = `${kind}${i}`;
  return `<article class="clip ${kind}"><h3>${esc(d.title)}</h3>
<p class="meta">${esc(d.name)} &middot; ${mmss(d.seconds)} &middot; ${sizeMb(d.mp4)} MB</p>
<video controls preload="metadata" src="${esc(d.file)}" id="${id}"></video>
${problems}<ol class="chapters" data-video="${id}">
${items}
</ol></article>`;
};
const section = (title, kind, items, reelFile) => items.length ? `<section><h2>${esc(title)}</h2>
${reelFile && existsSync(join(outDir, reelFile)) ? `<article class="clip ${kind}"><h3>Reel (all ${esc(kind)} tours)</h3><video controls preload="metadata" src="${esc(reelFile)}"></video></article>` : ''}
${items.map((d, i) => clip(d, i, kind)).join('\n')}
</section>` : '';
const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Borealis tours ${esc(date)}</title>
<style>
body{font:15px/1.5 -apple-system,Helvetica,Arial,sans-serif;max-width:1100px;margin:24px auto;padding:0 16px;background:#16181c;color:#e6e6e6}
h1{font-size:22px}h2{font-size:19px;margin:36px 0 4px;border-bottom:1px solid #333;padding-bottom:4px}h3{font-size:16px;margin:18px 0 2px}.meta{color:#9aa;margin:0 0 8px}
video{width:100%;background:#000;border-radius:6px}.chapters{columns:2;column-gap:32px;padding-left:22px}
.clip.ios video,.clip.android video{width:auto;max-width:100%;max-height:78vh;display:block}
a{color:#e0b040}.warn{color:#f08060}@media(max-width:700px){.chapters{columns:1}}
</style></head><body>
<h1>Borealis tours, ${esc(date)}</h1>
${section('Web (mock backend)', 'web', web, 'web-reel.mp4')}
${mobile.map((s) => section(s.label, s.platform, s.items, null)).join('\n')}
${section('macOS desktop (real app)', 'macos', macosItems, null)}
${mobileAll.length ? `<section><h2>Mobile reel (iOS, then Android)</h2><article class="clip mobile"><video controls preload="metadata" src="mobile-reel.mp4" style="width:auto;max-width:100%;max-height:78vh;display:block"></video></article></section>` : ''}
<script>
document.querySelectorAll('.chapters a').forEach(function (a) {
  a.addEventListener('click', function (e) {
    e.preventDefault();
    var v = document.getElementById(a.closest('ol').dataset.video);
    v.currentTime = parseFloat(a.dataset.seek); v.play();
  });
});
</script></body></html>
`;
writeFileSync(join(outDir, 'index.html'), html);
console.log(`wrote ${join(outDir, 'index.html')}`);
