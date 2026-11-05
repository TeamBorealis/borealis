/** Scrollbar width of the app (`--scrollbar`), repeated here because the iframe cannot read the host's tokens. */
const SCROLLBAR_PX = 17;
const ARROW_UP = "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='9' height='5'%3E%3Cpath d='M0 5L4.5 0L9 5Z' fill='%23686868'/%3E%3C/svg%3E\")";
const ARROW_DOWN = "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='9' height='5'%3E%3Cpath d='M0 0L4.5 5L9 0Z' fill='%23686868'/%3E%3C/svg%3E\")";

export type PreviewPaper = "letter" | "a4";

/** Printed page widths in CSS px (1 pt = 4/3 px): the renderer's `.letter{width:612pt}` and `.a4{width:595pt}`. */
const PAGE_PX: Record<PreviewPaper, number> = { letter: 612 * (4 / 3), a4: 595 * (4 / 3) };

/** Frame widths the scale steps cover (CSS px); a narrower frame keeps the first step, a wider one the last. */
const STEP_FROM_PX = 240;
const STEP_TO_PX = 2400;
const STEP_PX = 10;

/**
 * B-145: the page keeps its printed width and is scaled to the frame as a whole, never reflowed narrower (a reflowed page
 * overlapped its HP, hit dice and death saves boxes and cut Size and Speed). The iframe is sandboxed without scripts, so
 * the scale comes from width steps: at each step the page is as wide as the frame minus the scrollbar, never wider.
 */
export function previewZoom(frameWidth: number, paper: PreviewPaper): number {
  return Math.max(0.05, (frameWidth - SCROLLBAR_PX) / PAGE_PX[paper]);
}

/** Rounded down, so the drawn page never passes the frame. */
const zoomText = (frameWidth: number, paper: PreviewPaper): string => (Math.floor(previewZoom(frameWidth, paper) * 10000) / 10000).toString();

function scaleCss(paper: PreviewPaper): string {
  const rules = [`body.sheet .desk{zoom:${zoomText(STEP_FROM_PX, paper)};padding:12px 0}`];
  for (let w = STEP_FROM_PX + STEP_PX; w <= STEP_TO_PX; w += STEP_PX) {
    rules.push(`@media(min-width:${w}px){body.sheet .desk{zoom:${zoomText(w, paper)}}}`);
  }
  return rules.join("\n");
}

/**
 * Presentation CSS for the sandboxed preview: the generated sheet pages sit on a dark canvas with the app's own scrollbar.
 * It only restyles the container; the pages keep the renderer's printed size and look.
 */
export const PREVIEW_CSS = `
html{background:#131313}
body.sheet{background:#131313;margin:0}
body.sheet .page{margin:0 auto 12px;box-shadow:none;border-radius:0}
::-webkit-scrollbar{width:${SCROLLBAR_PX}px;height:${SCROLLBAR_PX}px;background:#131313}
::-webkit-scrollbar-track{background:#131313}
::-webkit-scrollbar-thumb{background:#686868;border:4px solid #131313;border-radius:0}
::-webkit-scrollbar-thumb:hover{background:#9e9e9e}
::-webkit-scrollbar-button:vertical:start:decrement,::-webkit-scrollbar-button:vertical:end:increment{display:block;width:${SCROLLBAR_PX}px;height:${SCROLLBAR_PX}px;background:#131313 no-repeat center / 9px 5px}
::-webkit-scrollbar-button:vertical:start:decrement{background-image:${ARROW_UP}}
::-webkit-scrollbar-button:vertical:end:increment{background-image:${ARROW_DOWN}}
`;

/** The paper of a generated sheet document (its pages carry `letter` or `a4`). */
export function paperOf(html: string): PreviewPaper {
  return /class="page a4\b/.test(html) ? "a4" : "letter";
}

/** Insert the preview CSS into a generated sheet document (before `</head>`; prepended when the document has no head). */
export function framePreviewHtml(html: string): string {
  const style = `<style>${PREVIEW_CSS}${scaleCss(paperOf(html))}</style>`;
  const at = html.indexOf("</head>");
  return at >= 0 ? `${html.slice(0, at)}${style}${html.slice(at)}` : `${style}${html}`;
}
