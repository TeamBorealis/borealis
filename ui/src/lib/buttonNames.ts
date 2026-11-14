/**
 * WKWebView (the macOS Tauri webview) exposes no accessible name for a <button> whose only name source is its
 * text content: the AX tree shows an unnamed button and no text child, so VoiceOver-style clients and the macOS e2e
 * engine cannot find "Skip", "Create character", ... Mirroring the visible text into aria-label fixes that without
 * touching markup or visuals. Buttons that already carry aria-label/aria-labelledby are left alone, and a label this
 * module set follows later text changes. Only installed inside the Tauri shell.
 */
const AUTO = "data-auto-label";

/** The button's text without the parts hidden with aria-hidden (B-144's Generate button holds two labels). */
function visibleText(node: Node): string {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? "";
  if (node instanceof Element && node.getAttribute("aria-hidden") === "true") return "";
  let text = "";
  node.childNodes.forEach((child) => { text += visibleText(child); });
  return text;
}

function nameOf(button: HTMLButtonElement): void {
  const own = button.hasAttribute("aria-label") && !button.hasAttribute(AUTO);
  if (own || button.hasAttribute("aria-labelledby")) return;
  const text = visibleText(button).replace(/\s+/g, " ").trim();
  if (text === "") {
    if (button.hasAttribute(AUTO)) { button.removeAttribute("aria-label"); button.removeAttribute(AUTO); }
    return;
  }
  if (button.getAttribute("aria-label") !== text) button.setAttribute("aria-label", text);
  button.setAttribute(AUTO, "");
}

export function nameButtons(root: ParentNode): void {
  root.querySelectorAll("button").forEach(nameOf);
}

/** Names every button now and whenever the DOM changes (batched per animation frame). Returns the disposer. */
export function installButtonNames(doc: Document = document): () => void {
  let queued = false;
  const run = () => { queued = false; nameButtons(doc); };
  const observer = new MutationObserver(() => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(run);
  });
  run();
  observer.observe(doc.body, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["aria-hidden"] });
  return () => observer.disconnect();
}
