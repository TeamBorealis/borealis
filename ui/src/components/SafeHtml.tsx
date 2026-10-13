const ALLOWED = new Set(["P", "UL", "OL", "LI", "B", "STRONG", "I", "EM", "BR", "H3", "H4", "TABLE", "THEAD", "TBODY", "TR", "TH", "TD", "SPAN", "BLOCKQUOTE", "CODE"]);

/** Defensive client-side allow-list sanitiser (the backend sanitises too). */
export function sanitizeHtml(html: string): string {
  if (typeof DOMParser === "undefined") return "";
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  const walk = (node: Element) => {
    for (const child of [...node.children]) {
      if (!ALLOWED.has(child.tagName)) {
        child.replaceWith(document.createTextNode(child.textContent ?? ""));
        continue;
      }
      for (const attr of [...child.attributes]) child.removeAttribute(attr.name);
      walk(child);
    }
  };
  walk(doc.body);
  return doc.body.innerHTML;
}

export function SafeHtml({ html }: { html: string }) {
  return <div className="rich" dangerouslySetInnerHTML={{ __html: sanitizeHtml(html) }} />;
}
