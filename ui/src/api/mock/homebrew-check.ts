import type {
  ElementSummary, HomebrewBlock, HomebrewBlockSpec, HomebrewExprNode, HomebrewForm, HomebrewPickerRow, HomebrewPickerTarget, HomebrewProblem, Page,
} from "../types";

// The web mock's stand-ins for what WB-06/WB-07 do on the backend: picker rows (`homebrew_reference_options`), the live
// problems of blocks and conditions, and the description preview. Enough for the creator's controls to be used and tested
// on the mock; the real answers come from `borealis-authoring`.

/** "Magic Item" -> "magic-item": the creator's type names against the mock's element types. */
const typeKey = (t: string): string => t.trim().toLowerCase().replace(/\s+/g, "-");

/** An element's row as the reference picker shows it (spec 13.5). */
export function pickerRow(e: ElementSummary): HomebrewPickerRow {
  return {
    id: e.id, name: e.name, type: e.type, summary: e.shortDescription, prerequisites: e.prerequisites, sourceName: e.sourceName,
    edition: e.rulesets.length === 1 ? e.rulesets[0]! : null, draft: false,
    ...(e.level !== undefined ? { level: e.level } : {}), ...(e.school ? { school: e.school } : {}), ...(e.spell ? { spell: e.spell } : {}),
  };
}

/** Rows for a picker: loaded elements and the pack's own, by type, tags and the typed words; one page of them. */
export function referenceRows(loaded: readonly ElementSummary[], own: readonly HomebrewPickerRow[], target: HomebrewPickerTarget, query: string, page: { offset: number; limit: number }): Page<HomebrewPickerRow> {
  const types = new Set(target.types.map(typeKey));
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const tags = (target.tags ?? []).map((t) => t.toLowerCase());
  const loadedRows = loaded.filter((e) => (tags.length === 0 || (e.tags ?? []).some((t) => tags.includes(t.toLowerCase())))).map(pickerRow);
  const rows = [...own, ...loadedRows].filter((r) => (types.size === 0 || types.has(typeKey(r.type))) && words.every((w) => `${r.name} ${r.summary}`.toLowerCase().includes(w)));
  const sorted = rows.sort((a, b) => a.name.localeCompare(b.name));
  return { total: sorted.length, items: sorted.slice(page.offset, page.offset + page.limit) };
}

const empty = (v: unknown): boolean => v === undefined || v === null || v === "" || (Array.isArray(v) && v.length === 0);

/** Condition rows the builder's check refuses (hc-wb02 `BuilderProblem`), with their model paths. */
function conditionProblems(node: HomebrewExprNode<Record<string, unknown>>, path: string): { path: string; message: string }[] {
  if (node.node === "group") return node.items.flatMap((n, i) => conditionProblems(n as HomebrewExprNode<Record<string, unknown>>, `${path ? `${path}.` : ""}items.${i}`));
  const row = node.row;
  if ((row.row === "has" || row.row === "lacks") && !row.id) return [{ path, message: "Pick the element this condition tests." }];
  if (row.row === "ability" && (Number(row.min) < 1 || Number(row.min) > 30)) return [{ path: `${path}.min`, message: "An ability score is between 1 and 30." }];
  if (row.row === "classLevel" && !row.class) return [{ path: `${path}.class`, message: "Choose the class whose level counts." }];
  if (row.row === "feature" && !String(row.name ?? "").trim()) return [{ path: `${path}.name`, message: "Give the feature's name." }];
  return [];
}

/** Problems of a builder value at `path` (`fields.requirements`, `blocks.0.fields.requirements`). */
function exprProblems(v: unknown, path: string, elementId: string | null): HomebrewProblem[] {
  const model = (v as { value?: { shape?: string; model?: unknown } } | null)?.value;
  if (!model || model.shape !== "model" || typeof model.model !== "object" || model.model === null || !("node" in model.model)) return [];
  return conditionProblems(model.model as HomebrewExprNode<Record<string, unknown>>, "").map((p) => ({
    severity: "error", message: p.message, path: p.path ? `${path}.${p.path}` : path, elementId, code: "builder",
  }));
}

/** A block's required fields and its condition (WB-07's `blocks.N.fields.<key>` sentences). */
function blockProblems(b: HomebrewBlock, i: number, catalog: readonly HomebrewBlockSpec[], elementId: string | null): HomebrewProblem[] {
  const spec = catalog.find((s) => s.kind === b.kind);
  if (!spec) return [];
  const missing = spec.fields.filter((f) => f.required && f.control !== "auto" && empty(b.fields[f.key])).map((f): HomebrewProblem => ({
    severity: "error", message: `${spec.label}: fill in ${f.label.toLowerCase()}.`, path: `blocks.${i}.fields.${f.key}`, elementId, code: "block.required",
  }));
  return [...missing, ...exprProblems(b.fields.requirements, `blocks.${i}.fields.requirements`, elementId)];
}

/** The mock's live problems of a form: the name, the prerequisite's rows, every block. */
export function formProblems(form: HomebrewForm, catalog: readonly HomebrewBlockSpec[], elementId: string | null): HomebrewProblem[] {
  const named = String(form.fields.name ?? "").trim() !== "";
  return [
    ...(named ? [] : [{ severity: "error" as const, message: "Give it a name.", path: "fields.name", elementId, code: "form.required" }]),
    ...exprProblems(form.fields.requirements, "fields.requirements", elementId),
    ...form.blocks.flatMap((b, i) => blockProblems(b, i, catalog, elementId)),
  ];
}

interface PmNode { type: string; text?: string; marks?: { type: string; attrs?: { tag?: string } }[]; content?: PmNode[]; attrs?: Record<string, unknown> }

const esc = (t: string): string => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

function inline(nodes: readonly PmNode[] = []): string {
  return nodes.map((n) => {
    if (n.type === "hard_break") return "<br />";
    let html = esc(n.text ?? "");
    for (const m of [...(n.marks ?? [])].reverse()) html = `<${m.attrs?.tag ?? (m.type === "bold" ? "b" : "i")}>${html}</${m.attrs?.tag ?? (m.type === "bold" ? "b" : "i")}>`;
    return html;
  }).join("");
}

function block(n: PmNode): string {
  switch (n.type) {
    case "paragraph": return `<p>${inline(n.content)}</p>`;
    case "heading": return `<h${n.attrs?.level === 4 ? 4 : 3}>${inline(n.content)}</h${n.attrs?.level === 4 ? 4 : 3}>`;
    case "bullet_list": case "ordered_list": {
      const tag = n.type === "bullet_list" ? "ul" : "ol";
      return `<${tag}>${(n.content ?? []).map((li) => `<li>${inline(li.content)}</li>`).join("")}</${tag}>`;
    }
    case "table": return `<table>${(n.content ?? []).map((tr) => `<tr>${(tr.content ?? []).map((c) => (c.type === "table_header" ? `<th>${inline(c.content)}</th>` : `<td>${inline(c.content)}</td>`)).join("")}</tr>`).join("")}</table>`;
    case "blockquote": return `<blockquote>${(n.content ?? []).map(block).join("")}</blockquote>`;
    case "kept": return String(n.attrs?.html ?? "");
    default: return "";
  }
}

/** The description preview: the mock stands in for the backend's one HTML writer (spec 7.2); plain text is a paragraph. */
export function descriptionHtml(v: unknown): string {
  if (typeof v === "string") return v.trim() ? `<p>${esc(v.trim())}</p>` : "";
  const doc = v as PmNode | null;
  return doc && doc.type === "doc" ? (doc.content ?? []).map(block).join("\n") : "";
}
