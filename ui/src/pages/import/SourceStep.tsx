import { type DragEvent, type ReactNode, useRef, useState } from "react";
import { IMPORT_LIMITS, type ImportHint, type ImportKind, type ImportSource, type ImportStartRequest, type Ruleset } from "../../api/contract";
import { Alert, Button, Icon, TextArea, useImportNotice } from "../../components";
import { Expander } from "../../components/controls";
import { DescHead, PaneSplit, Picker } from "../ai/Panes";
import { useApi } from "../../state/ApiContext";
import { browseNative, fileToSource, hasNativeDialog, problemWith, rulesContentInFile, type NativePick } from "./readSources";
import { PASTED_SUBJECT, rulesContentProblem } from "./rulesContent";

type KindChoice = ImportKind | "auto";
type EditionChoice = Ruleset | "auto";

const KINDS: { id: KindChoice; label: string }[] = [
  { id: "auto", label: "Auto" }, { id: "class", label: "Class" }, { id: "subclass", label: "Subclass" }, { id: "species", label: "Species" },
  { id: "feat", label: "Feat" }, { id: "spell", label: "Spell" }, { id: "item", label: "Item" }, { id: "monster", label: "Monster" },
];
const EDITIONS: { id: EditionChoice; label: string }[] = [{ id: "auto", label: "Auto" }, { id: "dnd2014", label: "2014" }, { id: "dnd2024", label: "2024" }];

export function SourceStep({ initialFiles, busy, onStart, bar }: { initialFiles: File[]; busy: boolean; onStart: (req: ImportStartRequest) => void; bar: ReactNode }) {
  const [files, setFiles] = useState<File[]>(initialFiles);
  const [pasted, setPasted] = useState("");
  const [described, setDescribed] = useState("");
  const [kind, setKind] = useState<KindChoice>("auto");
  const [edition, setEdition] = useState<EditionChoice>("auto");
  const [over, setOver] = useState(false);
  const api = useApi();
  const notice = useImportNotice();
  const [native, setNative] = useState<NativePick[]>([]);
  const [problem, setProblem] = useState<string | null>(null);
  const [reading, setReading] = useState(false);
  const picker = useRef<HTMLInputElement>(null);
  const camera = useRef<HTMLInputElement>(null);
  /** The content-import notice comes before any picker opens; Cancel leaves everything as it was. */
  const choose = async (input: React.RefObject<HTMLInputElement | null>) => { if (await notice.ensure()) input.current?.click(); };

  /** Aurora, 5etools and Borealis content is refused (it never goes to AI); null when every file may be imported. */
  const rulesContentIn = async (list: File[]): Promise<string | null> => {
    try { return (await Promise.all(list.map(rulesContentInFile))).find((p) => p) ?? null; }
    catch { return "One of the files could not be read. Remove it and try again."; }
  };
  const add = async (list: FileList | File[] | null) => {
    if (!list || !list.length) return;
    const incoming = Array.from(list);
    const bad = incoming.map(problemWith).find((p) => p) ?? (await rulesContentIn(incoming));
    setProblem(bad ?? (files.length + native.length + incoming.length > IMPORT_LIMITS.maxSources ? `Too many files at once (the limit is ${IMPORT_LIMITS.maxSources}).` : null));
    if (!bad) setFiles((cur) => [...cur, ...incoming]);
  };
  const browse = async () => {
    if (!(await notice.ensure())) return;
    try { const picked = await browseNative(api); if (picked.length) setNative((cur) => [...cur, ...picked]); }
    catch (e) { setProblem(e instanceof Error ? e.message : String(e)); }
  };
  const onDrop = (e: DragEvent) => { e.preventDefault(); e.stopPropagation(); setOver(false); void add(Array.from(e.dataTransfer.files)); };
  const hasFiles = files.length > 0 || native.length > 0;
  const notesLength = (pasted.trim() + described.trim()).length;
  const notesTooLong = hasFiles && notesLength > IMPORT_LIMITS.maxNotesChars;
  const ready = files.length > 0 || native.length > 0 || pasted.trim() !== "" || described.trim() !== "";

  const submit = async () => {
    if (!(await notice.ensure())) return;
    // Files parked by a window-wide drop skip `add`, so everything is checked again here.
    const refused = [pasted, described].map((t) => rulesContentProblem(t, PASTED_SUBJECT)).find((p) => p) ?? (await rulesContentIn(files));
    if (refused) { setProblem(refused); return; }
    setReading(true);
    let read: ImportSource[];
    try { read = await Promise.all(files.map(fileToSource)); }
    catch { setProblem("One of the files could not be read. Remove it and try again."); setReading(false); return; }
    setReading(false);
    const sources: ImportSource[] = [
      ...read,
      ...native.map((n) => n.source),
      ...(pasted.trim() ? [{ kind: "text" as const, text: pasted.trim() }] : []),
      ...(described.trim() ? [{ kind: "text" as const, text: described.trim() }] : []),
    ];
    const hint: ImportHint = { ...(kind !== "auto" ? { kind } : {}), ...(edition !== "auto" ? { edition } : {}) };
    onStart({ sources, ...(Object.keys(hint).length ? { hint } : {}) });
  };

  const list = (
    <>
      <Expander title="Sources">
        <div className="v5-inline">
          <div
            className={`v5-drop ${over ? "over" : ""}`}
            role="button" tabIndex={0} aria-label="Drop files here or press Enter to choose files"
            onClick={() => void choose(picker)}
            onKeyDown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); void choose(picker); } }}
            onDragOver={(e) => { e.preventDefault(); setOver(true); }} onDragLeave={() => setOver(false)} onDrop={onDrop}
          >
            <Icon name="upload" />
            <strong>Drop a PDF, a photo or a text file</strong>
            <span className="v5-note">or click to choose. Scans and screenshots are fine.</span>
          </div>
          <input ref={picker} className="sr-only" type="file" multiple accept=".pdf,.docx,.odt,.txt,.md,.json,application/pdf,application/vnd.openxmlformats-officedocument.wordprocessingml.document,application/vnd.oasis.opendocument.text,image/*,text/*" aria-label="Choose files" tabIndex={-1} onChange={(e) => { void add(e.target.files ? Array.from(e.target.files) : null); e.target.value = ""; }} />
          <div className="v5-foot">
            <Button className="camera-btn" icon="plus" onClick={() => void choose(camera)}>Take photo</Button>
            <input ref={camera} className="sr-only" type="file" accept="image/*" capture="environment" aria-label="Take photo" tabIndex={-1} onChange={(e) => { void add(e.target.files ? Array.from(e.target.files) : null); e.target.value = ""; }} />
            <Button icon="upload" onClick={() => void choose(picker)}>Choose files</Button>
            {hasNativeDialog() && <Button aria-label="Browse…" icon="scroll" onClick={() => void browse()}>Browse...</Button>}
          </div>
          {problem && <Alert tone="danger">{problem}</Alert>}
        </div>
        {hasFiles && (
          <ul className="v5-list" aria-label="Files to import">
            {files.map((f, i) => (
              <li key={`${f.name}-${i}`} className="v5-row static">
                <Icon name="scroll" /><span className="grow">{f.name}</span>
                <Button small variant="ghost" aria-label={`Remove ${f.name}`} onClick={() => setFiles((cur) => cur.filter((_, j) => j !== i))}>Remove</Button>
              </li>
            ))}
            {native.map((n, i) => (
              <li key={n.source.token} className="v5-row static">
                <Icon name="scroll" /><span className="grow">{n.name}</span>
                <Button small variant="ghost" aria-label={`Remove ${n.name}`} onClick={() => setNative((cur) => cur.filter((_, j) => j !== i))}>Remove</Button>
              </li>
            ))}
          </ul>
        )}
        <div className="v5-inline">
          {hasFiles ? (
            <>
              <TextArea
                label="Notes about these files (optional)" rows={3} value={described} onChange={(e) => setDescribed(e.target.value)}
                placeholder="e.g. this is a 2024 Fighter subclass, the table on the last page is its spell list"
              />
              <span className="v5-note">Your notes go along with the files as hints, up to {IMPORT_LIMITS.maxNotesChars} characters. {pasted.trim() ? "The text you pasted earlier is included." : ""}</span>
              {notesTooLong && <Alert tone="danger">The notes are too long ({notesLength} of {IMPORT_LIMITS.maxNotesChars} characters).</Alert>}
            </>
          ) : (
            <div className="v5-form">
              <TextArea label="Paste text" rows={5} value={pasted} onChange={(e) => setPasted(e.target.value)} placeholder="Paste the class, feat, spell or item text here" />
              <TextArea label="Describe it" rows={5} value={described} onChange={(e) => setDescribed(e.target.value)} placeholder="Or just describe what you want, e.g. a half-caster that etches runes on weapons" />
            </div>
          )}
        </div>
      </Expander>
      <Expander title="What is it? (optional)" footer={
        <div className="v5-inline"><div><Button variant="primary" icon="sparkles" disabled={!ready || busy || reading || notesTooLong} onClick={() => void submit()}>{busy || reading ? "Starting..." : "Import"}</Button></div></div>
      }>
        <div className="v5-inline">
          <Picker label="Content type" options={KINDS} value={kind} onChange={setKind} />
          <span className="field-label">Edition</span>
          <Picker label="Edition" options={EDITIONS} value={edition} onChange={setEdition} />
        </div>
      </Expander>
    </>
  );
  const desc = (
    <>
      <DescHead title="Import anything" lead="Turn a PDF, a photo or a few lines of text into content you can use on a character." />
      <h2>What you can add</h2>
      <p>Classes, subclasses, species, feats, spells, items and monsters. Scans and screenshots are fine, and so is a plain description of what you want.</p>
      <h2>What happens next</h2>
      <p>Borealis reads the source, writes the content, tests it in the rules engine and shows each element for review before anything is added to your library.</p>
      <h2>Privacy</h2>
      <p>Everything stays on this device.</p>
    </>
  );
  return <PaneSplit list={<>{bar}{list}</>} desc={desc} label="Import" />;
}
