/** Scrollbar width of the app (`--scrollbar`), repeated here because the iframe cannot read the host's tokens. */
const SCROLLBAR_PX = 17;
const LETTER_RATIO = 11 / 8.5;
const ARROW_UP = "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='9' height='5'%3E%3Cpath d='M0 5L4.5 0L9 5Z' fill='%23686868'/%3E%3C/svg%3E\")";
const ARROW_DOWN = "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='9' height='5'%3E%3Cpath d='M0 0L4.5 5L9 0Z' fill='%23686868'/%3E%3C/svg%3E\")";

/**
 * Presentation CSS for the sandboxed preview: the generated sheet pages become full-width white pages on a dark canvas,
 * with the app's own scrollbar. It only restyles the container; the sheet markup and its look come from the Rust renderer.
 */
export const PREVIEW_CSS = `
html{background:#131313}
body.sheet{background:#131313;margin:0}
body.sheet .page{width:auto;max-width:none;margin:0 0 12px;box-shadow:none;border-radius:0;min-height:calc((100vw - ${SCROLLBAR_PX}px) * ${LETTER_RATIO.toFixed(4)})}
::-webkit-scrollbar{width:${SCROLLBAR_PX}px;height:${SCROLLBAR_PX}px;background:#131313}
::-webkit-scrollbar-track{background:#131313}
::-webkit-scrollbar-thumb{background:#686868;border:4px solid #131313;border-radius:0}
::-webkit-scrollbar-thumb:hover{background:#9e9e9e}
::-webkit-scrollbar-button:vertical:start:decrement,::-webkit-scrollbar-button:vertical:end:increment{display:block;width:${SCROLLBAR_PX}px;height:${SCROLLBAR_PX}px;background:#131313 no-repeat center / 9px 5px}
::-webkit-scrollbar-button:vertical:start:decrement{background-image:${ARROW_UP}}
::-webkit-scrollbar-button:vertical:end:increment{background-image:${ARROW_DOWN}}
`;

/** Insert the preview CSS into a generated sheet document (before `</head>`; prepended when the document has no head). */
export function framePreviewHtml(html: string): string {
  const style = `<style>${PREVIEW_CSS}</style>`;
  const at = html.indexOf("</head>");
  return at >= 0 ? `${html.slice(0, at)}${style}${html.slice(at)}` : `${style}${html}`;
}
