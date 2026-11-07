import { describe, expect, it } from "vitest";
import { noteRefs, renderNote, sanitizeNoteHtml, type RenderContext } from "../pages/notes/markdown/render";
import { parseProperties, propertyTags, splitFrontMatter } from "../pages/notes/markdown/frontMatter";
import { extractSection, toggleTask } from "../pages/notes/markdown/sections";
import { parseWikiLink } from "../pages/notes/markdown/obsidian";

const PNG = "data:image/png;base64,iVBORw0KGgo=";

function ctx(over: Partial<RenderContext> = {}): RenderContext {
  return { from: "Home.md", resolved: new Map(), images: new Map(), notes: new Map(), depth: 0, ...over };
}
function dom(text: string, over: Partial<RenderContext> = {}): HTMLElement {
  const el = document.createElement("div");
  el.innerHTML = renderNote(text, ctx(over));
  return el;
}

describe("B-102 reading view: Obsidian syntax", () => {
  it("hides front matter and keeps the body's headings, code and tables", () => {
    const el = dom("---\ntags: [a]\n---\n# Title\n\n## Part\n\n```js\nconst x = 1;\n```\n\n| A | B |\n|:--|--:|\n| 1 | 2 |\n");
    expect(el.textContent).not.toContain("tags:");
    expect(el.querySelector("h1")?.textContent).toBe("Title");
    expect(el.querySelector("h2")?.getAttribute("data-heading")).toBe("Part");
    expect(el.querySelector("pre code")?.textContent).toBe("const x = 1;\n");
    expect(el.querySelectorAll("table td")).toHaveLength(2);
    expect(el.querySelector("td.align-right")?.textContent).toBe("2");
  });

  it("renders wikilinks with aliases, headings and blocks, and marks unresolved ones", () => {
    const resolved = new Map<string, string | null>([["Harbor Town", "Places/Harbor Town.md"], ["Bramblewick", "People/Bramblewick.md"], ["Missing", null]]);
    const el = dom("[[Harbor Town|the harbor]] [[Bramblewick#Appearance]] [[Session 1#^loot]] [[Missing]] [[#Top]]", { resolved });
    const links = [...el.querySelectorAll<HTMLAnchorElement>("a.md-wikilink")];
    expect(links.map((a) => a.textContent)).toEqual(["the harbor", "Bramblewick > Appearance", "Session 1 > loot", "Missing", "Top"]);
    expect(links[0]!.dataset.wikilink).toBe("Harbor Town");
    expect(links[1]!.dataset.subpath).toBe("Appearance");
    expect(links[2]!.dataset.subpath).toBe("^loot");
    expect(links[3]!.classList.contains("is-unresolved")).toBe(true);
    expect(links[0]!.classList.contains("is-unresolved")).toBe(false);
    expect(links.every((a) => a.getAttribute("href") === "#")).toBe(true);
  });

  it("embeds images and one level of notes, sections included", () => {
    const resolved = new Map<string, string | null>([["map.png", "map.png"], ["Bramblewick", "People/Bramblewick.md"]]);
    const notes = new Map([["People/Bramblewick.md", "# Bramblewick\n\nIntro ![[Bramblewick]]\n\n## Appearance\n\nCopper goggles.\n\n## Notes\n\nLater."]]);
    const el = dom("![[map.png|120]]\n\n![[Bramblewick#Appearance]]", { resolved, images: new Map([["map.png", PNG]]), notes });
    const img = el.querySelector("img.md-embed-image")!;
    expect(img.getAttribute("src")).toBe(PNG);
    expect(img.getAttribute("width")).toBe("120");
    const embed = el.querySelector(".md-embed")!;
    expect(embed.querySelector(".md-embed-title")?.textContent).toBe("Bramblewick > Appearance");
    expect(embed.querySelector(".md-embed-body")?.textContent).toContain("Copper goggles.");
    expect(embed.querySelector(".md-embed-body")?.textContent).not.toContain("Later.");
    // The whole note embedded: its own embed of itself is a link, never a second level.
    const whole = dom("![[Bramblewick]]", { resolved, notes });
    expect(whole.querySelectorAll(".md-embed")).toHaveLength(1);
    expect(whole.querySelector(".md-embed .md-embed-body a.md-wikilink")?.textContent).toBe("Bramblewick");
  });

  it("renders tags from the text and leaves code and headings alone", () => {
    const el = dom("A #npc and #place/town, not#this, `#code`.\n\n#2024 alone\n\n# Heading");
    expect([...el.querySelectorAll<HTMLElement>("a.tag")].map((a) => a.dataset.tag)).toEqual(["npc", "place/town"]);
    expect(el.querySelector("h1")?.textContent).toBe("Heading");
  });

  it("renders callouts, foldable ones as details", () => {
    const el = dom("> [!tip] Table rule\n> Snacks rotate.\n\n> [!warning]- Spoilers\n> Hidden text.\n\n> [!note]+\n> Open one.\n\n> Plain quote.");
    const tip = el.querySelector('[data-callout="tip"]')!;
    expect(tip.tagName).toBe("DIV");
    expect(tip.querySelector(".callout-title")?.textContent).toBe("Table rule");
    expect(tip.querySelector(".callout-content")?.textContent).toContain("Snacks rotate.");
    const warning = el.querySelector<HTMLDetailsElement>('details[data-callout="warning"]')!;
    expect(warning.open).toBe(false);
    expect(warning.querySelector("summary")?.textContent).toBe("Spoilers");
    const note = el.querySelector<HTMLDetailsElement>('details[data-callout="note"]')!;
    expect(note.open).toBe(true);
    expect(note.querySelector("summary")?.textContent).toBe("Note");
    expect(el.querySelector("blockquote")?.textContent).toContain("Plain quote.");
  });

  it("renders task lists with checkboxes that know their source line", () => {
    const text = "---\na: 1\n---\n- [ ] One\n- [x] Two\n  - [ ] Nested\n> - [ ] Quoted";
    const boxes = [...dom(text).querySelectorAll<HTMLInputElement>("input.task-checkbox")];
    expect(boxes.map((b) => [b.dataset.taskLine, b.checked])).toEqual([["3", false], ["4", true], ["5", false], ["6", false]]);
    expect(dom(text).querySelector("li.task-list-item")?.textContent?.trim()).toBe("One");
  });

  it("disables the checkboxes of an embedded note, whose lines are not this note's", () => {
    const resolved = new Map<string, string | null>([["Chores", "Chores.md"]]);
    const notes = new Map([["Chores.md", "- [ ] Theirs\n- [x] Done"]]);
    const el = dom("- [ ] Mine\n\n![[Chores]]", { resolved, notes });
    const boxes = [...el.querySelectorAll<HTMLInputElement>("input.task-checkbox")];
    expect(boxes.map((b) => [b.dataset.taskLine ?? null, b.disabled])).toEqual([["0", false], [null, true], [null, true]]);
  });

  it("hides block ids but keeps them for block links", () => {
    const el = dom("A chest of coins. ^loot");
    expect(el.textContent).not.toContain("^loot");
    expect(el.querySelector("p")?.getAttribute("data-block")).toBe("loot");
  });

  it("shows plugin code blocks as code", () => {
    const el = dom("```dataview\nTABLE x FROM #tag\n```");
    expect(el.querySelector("pre code.language-dataview")?.textContent).toContain("TABLE x FROM #tag");
    expect(el.querySelector("a.tag")).toBeNull();
  });

  it("keeps web links as backend-opened links and vault Markdown links as note links", () => {
    const resolved = new Map<string, string | null>([["Places/Harbor Town.md", "Places/Harbor Town.md"]]);
    const el = dom("[web](https://example.org/a) https://example.org/b [town](Places/Harbor%20Town.md#Docks)", { resolved });
    const ext = [...el.querySelectorAll<HTMLAnchorElement>("a.md-external")];
    expect(ext.map((a) => a.dataset.external)).toEqual(["https://example.org/a", "https://example.org/b"]);
    const town = el.querySelector<HTMLAnchorElement>("a.md-wikilink")!;
    expect(town.dataset.wikilink).toBe("Places/Harbor Town.md");
    expect(town.dataset.subpath).toBe("Docks");
  });

  it("sanitises every kind of HTML payload", () => {
    const evil = [
      "<script>alert(1)</script>",
      '<img src="x" onerror="alert(2)">',
      '<a href="javascript:alert(3)">x</a>',
      '<iframe src="https://example.org"></iframe>',
      '<div style="position:fixed">overlay</div>',
      '<input type="text" value="phish">',
      '<form action="https://example.org"><button>Go</button></form>',
      "[click](javascript:alert(4))",
      '<svg><script>alert(5)</script></svg>',
      '<img src="file:///etc/passwd">',
      '<a href="https://example.org" onclick="alert(6)">raw</a>',
    ].join("\n\n");
    const el = dom(evil);
    expect(el.querySelector("script, iframe, form, button, svg, [onerror], [onclick], [style], input")).toBeNull();
    const values = [...el.querySelectorAll("*")].flatMap((n) => [...n.attributes].map((a) => a.value));
    expect(values.some((v) => /javascript:|file:/i.test(v))).toBe(false);
    expect([...el.querySelectorAll("a")].every((a) => a.getAttribute("href") === "#")).toBe(true);
    expect(el.querySelector("img")?.hasAttribute("src")).toBe(false);
    expect(el.querySelector<HTMLAnchorElement>("a[data-external]")?.dataset.external).toBe("https://example.org");
    expect(sanitizeNoteHtml('<img src="data:image/png;base64,AAAA">')).toContain("data:image/png");
  });

  it("collects the link targets to resolve and the embeds to load", () => {
    const refs = noteRefs("[[A]] ![[b.png]] [[A#x]] [m](Places/C%20D.md) ![](e.png) [w](https://x.org) `[[code]]`");
    expect(refs.targets).toEqual(["A", "b.png", "Places/C D.md", "e.png"]);
    expect(refs.embeds).toEqual(["b.png", "e.png"]);
  });
});

describe("B-102 front matter, sections and tasks", () => {
  it("splits front matter like the backend and reads properties", () => {
    const fm = splitFrontMatter("---\ntags: [a, b]\ndate: 2026-10-02\nnested: {x: 1}\n---\nBody")!;
    expect(fm.endLine).toBe(5);
    const props = parseProperties(fm.yaml);
    expect(props.error).toBeNull();
    expect(props.entries).toEqual([["tags", ["a", "b"]], ["date", "2026-10-02"], ["nested", { x: 1 }]]);
    expect(splitFrontMatter("\n---\na: 1\n---\n")).toBeNull();
    expect(splitFrontMatter("---\nno end")).toBeNull();
    expect(parseProperties("a: [broken").error).not.toBeNull();
    expect(propertyTags("one, two #three")).toEqual(["one", "two", "three"]);
    expect(propertyTags(["#x", "y"])).toEqual(["x", "y"]);
  });

  it("toggles a task by changing exactly one character", () => {
    const text = "# T\n- [ ] One\n  * [x] Two\n> 1. [/] Three\nplain [ ] text";
    expect(toggleTask(text, 1)).toBe("# T\n- [x] One\n  * [x] Two\n> 1. [/] Three\nplain [ ] text");
    expect(toggleTask(text, 2)).toBe("# T\n- [ ] One\n  * [ ] Two\n> 1. [/] Three\nplain [ ] text");
    expect(toggleTask(text, 3)).toBe("# T\n- [ ] One\n  * [x] Two\n> 1. [ ] Three\nplain [ ] text");
    expect(toggleTask(text, 4)).toBeNull();
    expect(toggleTask(text, 99)).toBeNull();
  });

  it("extracts heading sections and blocks for embeds", () => {
    const note = "---\na: 1\n---\n# Top\nIntro\n## Appearance\nGoggles\n### Detail\nInk\n## Notes\nLater\n\nFirst line\nA chest. ^loot\n";
    expect(extractSection(note, null).startsWith("# Top")).toBe(true);
    expect(extractSection(note, "appearance")).toBe("## Appearance\nGoggles\n### Detail\nInk");
    expect(extractSection(note, "^loot")).toBe("First line\nA chest. ^loot");
    expect(extractSection(note, "Unknown")).toContain("# Top");
  });

  it("parses wikilink parts like the backend", () => {
    expect(parseWikiLink("Folder/Note#Head|Alias", false)).toEqual({ embed: false, target: "Folder/Note", subpath: "Head", alias: "Alias" });
    expect(parseWikiLink("Note\\|alias", false)).toEqual({ embed: false, target: "Note", subpath: null, alias: "alias" });
    expect(parseWikiLink("#^block", true)).toEqual({ embed: true, target: "", subpath: "^block", alias: null });
  });
});
