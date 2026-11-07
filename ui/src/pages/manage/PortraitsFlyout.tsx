import { CheckBox } from "../../components/controls";
import { useRef, useState } from "react";
import { Alert, Button, Flyout } from "../../components";
import { mdiFolderImage } from "@mdi/js";
import { fileToDataUrl } from "../../lib/files";
import { useApi } from "../../state/ApiContext";
import { useAsync } from "../../state/useAsync";
import { PORTRAIT_LIBRARY_UNAVAILABLE } from "../../components/shell/useFolderNotice";
import { PortraitLibraryTiles, libraryNote, type TileActions } from "./PortraitLibraryTiles";

const KEY = "borealis.portraits";
const MAX = 12;

const load = (): string[] => {
  try {
    const raw = localStorage.getItem(KEY);
    const v: unknown = raw ? JSON.parse(raw) : [];
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string").slice(0, MAX) : [];
  } catch {
    return [];
  }
};
const store = (list: string[]) => { try { localStorage.setItem(KEY, JSON.stringify(list)); } catch { /* storage full or unavailable: gallery is per-session then */ } };

/**
 * Portraits gallery: images the user has provided on this device (no bundled art), then the images of the portrait library
 * folder chosen in Settings > Content (GitHub #32), in the same grid, as the classic builder lists its portraits folder.
 */
export function PortraitsFlyout({ current, onSelect, onClose }: { current: string | null; onSelect: (dataUrl: string) => void; onClose: () => void }) {
  const api = useApi();
  const library = useAsync(() => api.listPortraitLibrary(), [api]);
  const [gallery, setGallery] = useState<string[]>(() => (current && !load().includes(current) ? [current, ...load()] : load()).slice(0, MAX));
  const [picked, setPicked] = useState<string | null>(current);
  const [error, setError] = useState<string | null>(null);
  // Gallery images are user-provided and carry no gender tag, so every image stays visible whatever these filters say.
  const [showAll, setShowAll] = useState(false);
  const [male, setMale] = useState(true);
  const [female, setFemale] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const select = (url: string) => { onSelect(url); onClose(); };
  const actions: TileActions = { picked, pick: setPicked, select };
  const onFile = async (f: File | undefined) => {
    if (!f) return;
    try {
      const url = await fileToDataUrl(f);
      const next = [url, ...gallery.filter((g) => g !== url)].slice(0, MAX);
      setGallery(next);
      store(next);
      setPicked(url);
      setError(null);
    } catch {
      setError("That image could not be read.");
    }
  };
  const lib = library.data ?? null;
  const empty = gallery.length === 0 && !lib?.images.length;
  const note = libraryNote(lib);
  return (
    <Flyout title="Portraits gallery" width="portraits" onClose={onClose} footer={
      <div className="pg-foot">
        <div className="pg-filters">
          <label><CheckBox checked={showAll} onChange={(e) => setShowAll(e.target.checked)} />Show all</label>
          <span>
            <label><CheckBox checked={male} onChange={(e) => setMale(e.target.checked)} />Male</label>
            <label><CheckBox checked={female} onChange={(e) => setFemale(e.target.checked)} />Female</label>
          </span>
        </div>
        <div className="pg-buttons">
          <Button variant="flat" disabled={!picked} onClick={() => { if (picked) select(picked); }}>Select portrait</Button>
          <Button variant="flat" onClick={() => fileRef.current?.click()}><svg className="icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d={mdiFolderImage} /></svg>Browse</Button>
          <Button variant="flat" data-autofocus onClick={onClose}>Cancel</Button>
        </div>
      </div>
    }>
      <input ref={fileRef} type="file" accept="image/png,image/jpeg" hidden aria-label="Add a portrait image" onChange={(e) => { void onFile(e.target.files?.[0]); e.target.value = ""; }} />
      <div className="pg-grid" role="group" aria-label="Portraits">
        {gallery.map((g, i) => (
          <button key={g.slice(-24) + i} type="button" className="pg-tile" aria-pressed={g === picked} aria-label={`Portrait ${i + 1}`} onClick={() => setPicked(g)} onDoubleClick={() => select(g)}>
            <img src={g} alt="" />
          </button>
        ))}
        <PortraitLibraryTiles library={lib} actions={actions} />
        {empty && Array.from({ length: 3 }, (_, i) => <div key={i} className="pg-tile empty" aria-hidden="true">Empty</div>)}
      </div>
      <p className="pg-note">{gallery.length === 0 ? "Use Browse to add your own portraits. They stay on this device." : "Images you add stay on this device."}</p>
      {note && <p className="pg-note">{note}</p>}
      {lib?.unavailable && <Alert tone="warning" role="status">{PORTRAIT_LIBRARY_UNAVAILABLE}</Alert>}
      {error && <p className="error-text" role="alert">{error}</p>}
    </Flyout>
  );
}
