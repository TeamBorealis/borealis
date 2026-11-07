import { useEffect, useRef, useState } from "react";
import type { PortraitLibrary, PortraitLibraryImage } from "../../api/contract";
import { useApi } from "../../state/ApiContext";
import { errorMessage } from "../../state/useAsync";

/** Picking and selecting a tile: the image's `data:` URL, as the gallery's own tiles hold. */
export interface TileActions {
  picked: string | null;
  pick: (dataUrl: string) => void;
  select: (dataUrl: string) => void;
}

const label = (image: PortraitLibraryImage) => `Library portrait ${image.folder ? `${image.folder}/` : ""}${image.name}`;

/**
 * Load `run` once the element is near the viewport (IntersectionObserver), or at once where there is none (tests). A large
 * library then reads only the images the user scrolls to, one backend call each.
 */
function useWhenVisible(run: () => void) {
  const ref = useRef<HTMLButtonElement>(null);
  const runRef = useRef(run);
  runRef.current = run;
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof IntersectionObserver === "undefined") { runRef.current(); return; }
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) { io.disconnect(); runRef.current(); }
    }, { rootMargin: "200px" });
    io.observe(el);
    return () => io.disconnect();
  }, []);
  return ref;
}

/** One library image: read through the backend (the web view never reads the folder), then picked like a gallery image. */
function LibraryTile({ image, actions }: { image: PortraitLibraryImage; actions: TileActions }) {
  const api = useApi();
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const ref = useWhenVisible(() => {
    api.portraitLibraryImage({ id: image.id }).then(setUrl, (e) => setFailed(errorMessage(e)));
  });
  return (
    <button ref={ref} type="button" className="pg-tile" aria-label={label(image)} title={failed ?? image.name}
      aria-pressed={url !== null && url === actions.picked} disabled={failed !== null}
      onClick={() => { if (url) actions.pick(url); }} onDoubleClick={() => { if (url) actions.select(url); }}>
      {url ? <img src={url} alt="" /> : <span aria-hidden="true">{failed ? "?" : ""}</span>}
    </button>
  );
}

/** The library's tiles, after the gallery's own (the classic builder lists its portraits folder in the same grid). */
export function PortraitLibraryTiles({ library, actions }: { library: PortraitLibrary | null; actions: TileActions }) {
  return <>{(library?.images ?? []).map((image) => <LibraryTile key={image.id} image={image} actions={actions} />)}</>;
}

/** The note under the grid about the library: how many images, what was left out. */
export function libraryNote(library: PortraitLibrary | null): string | null {
  if (!library || library.unavailable || !library.dir) return null;
  const n = library.images.length;
  const parts = [n === 0 ? "Your portrait library folder has no images." : `${n} image${n === 1 ? "" : "s"} from your portrait library.`];
  if (library.truncated) parts.push(`Only the first ${n} are shown.`);
  if (library.refused > 0) {
    parts.push(`${library.refused} image${library.refused === 1 ? "" : "s"} could not be used (larger than 2 MiB, or not a PNG, JPEG, GIF or WebP image).`);
  }
  return parts.join(" ");
}
