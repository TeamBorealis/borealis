import type { Node as PMNode } from "prosemirror-model";
import type { EditorView, NodeView } from "prosemirror-view";
import { sanitizeHtml } from "../../../components/SafeHtml";
import type { KeptReason } from "./types";

// A kept block (homebrew creator spec 7.3) in the editor: the original HTML shown rendered and read-only under a
// "Kept as is" label, with a Remove button. Its bytes are written back unchanged, so nothing here edits them.

const REASON_TEXT: Record<KeptReason, string> = {
  outside_palette: "This part uses formatting the editor does not have. It is kept exactly as written.",
  not_canonical: "This part is written in a way the editor would rewrite. It is kept exactly as written.",
};

/** Builds the node view; `onRemove` gets the block's position. */
export function keptNodeView(onRemove: (pos: number) => void) {
  return (node: PMNode, _view: EditorView, getPos: () => number | undefined): NodeView => {
    const dom = document.createElement("div");
    dom.className = "rich-kept-block";
    dom.setAttribute("contenteditable", "false");

    const head = document.createElement("div");
    head.className = "rich-kept-head";
    const label = document.createElement("span");
    label.className = "rich-kept-label";
    label.textContent = "Kept as is";
    const reason = (node.attrs.reason as KeptReason) in REASON_TEXT ? (node.attrs.reason as KeptReason) : "outside_palette";
    label.title = REASON_TEXT[reason];
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "rich-kept-remove";
    remove.textContent = "Remove";
    remove.title = "Remove this kept part from the description";
    remove.setAttribute("aria-label", "Remove kept part");
    remove.addEventListener("mousedown", (e) => e.preventDefault());
    remove.addEventListener("click", () => {
      const pos = getPos();
      if (pos !== undefined) onRemove(pos);
    });
    head.append(label, remove);

    const body = document.createElement("div");
    body.className = "rich rich-kept";
    body.innerHTML = sanitizeHtml(String(node.attrs.html));

    dom.append(head, body);
    return {
      dom,
      stopEvent: (event) => event.target instanceof Element && event.target.closest("button") !== null,
      ignoreMutation: () => true,
    };
  };
}
