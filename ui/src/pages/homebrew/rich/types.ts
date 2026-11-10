// JSON form of a rich-text description: the ProseMirror JSON of `schema.ts`, and exactly the serde form of the Rust
// tree (`borealis_content::rich`, planned move to `borealis-authoring::rich`). Rust writes the HTML; the UI never does.

export type BoldTag = "b" | "strong";
export type ItalicTag = "i" | "em";

export type MarkJson = { type: "bold"; attrs: { tag: BoldTag } } | { type: "italic"; attrs: { tag: ItalicTag } };

export type InlineJson =
  | { type: "text"; text: string; marks?: MarkJson[] }
  | { type: "hard_break"; marks?: MarkJson[] };

export type ListItemJson = { type: "list_item"; content?: InlineJson[] };
export type CellJson = { type: "table_cell" | "table_header"; content?: InlineJson[] };
export type TableRowJson = { type: "table_row"; attrs: { head: boolean }; content?: CellJson[] };

export type KeptReason = "outside_palette" | "not_canonical";

export type BlockJson =
  | { type: "paragraph"; attrs: { joined: boolean; bare: boolean }; content?: InlineJson[] }
  | { type: "heading"; attrs: { joined: boolean; level: number }; content?: InlineJson[] }
  | { type: "bullet_list" | "ordered_list"; attrs: { joined: boolean }; content?: ListItemJson[] }
  | { type: "table"; attrs: { joined: boolean; sections: boolean }; content?: TableRowJson[] }
  | { type: "blockquote"; attrs: { joined: boolean }; content?: BlockJson[] }
  | { type: "kept"; attrs: { joined: boolean; html: string; reason: KeptReason } };

export interface DocJson {
  type: "doc";
  content?: BlockJson[];
}

export type EscapeKind = "backslash_escape" | "html_entity" | "mojibake" | "stray_backslash";

/** One escape artefact in a string. `start` is a UTF-16 index (the Rust twin reports a byte offset). */
export interface TextFinding {
  kind: EscapeKind;
  start: number;
  found: string;
  replacement: string;
}

/** One escape artefact in a document: `path` runs block, list item / row and cell / child block, inline node. */
export interface EscapeFinding {
  path: number[];
  kind: EscapeKind;
  found: string;
  replacement: string;
}
