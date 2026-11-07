import type { AppSettings, PortraitLibrary } from "../types";
import { apiError } from "./errors";
import type { MockPicks } from "./picks";

/** The eight PNG signature bytes, as a binary string for `btoa`. */
const PNG_SIGNATURE = "\x89PNG\r\n\x1a\n";

/** Mirrors `portrait_library.rs` (GitHub #32): a token-only folder, its images by id, and the unavailable state. */
export class MockPortraitLibrary {
  /** Library image ids (`name` or `folder/name`) in listing order; tests may replace them. */
  ids: string[] = ["elf-female.png", "human-male.jpg", "Dwarves/shield-dwarf.png"];
  /** Simulate an unplugged folder: the setting stays, nothing is listed. */
  unavailable = false;
  /** Image files the backend would refuse (too large, not an image). */
  refused = 0;

  constructor(private readonly picks: MockPicks) {}

  /** Returns the new settings for a `portrait-library-dir` token (or no library when absent). */
  set(s: AppSettings, token?: string): AppSettings {
    if (token === undefined) return { ...s, portraitLibraryDir: null };
    return { ...s, portraitLibraryDir: this.picks.take(token, "portrait-library-dir") };
  }

  isUnavailable(s: AppSettings): boolean {
    return s.portraitLibraryDir !== null && this.unavailable;
  }

  list(s: AppSettings): PortraitLibrary {
    const dir = s.portraitLibraryDir;
    if (dir === null || this.unavailable) return { dir, unavailable: this.isUnavailable(s), images: [], refused: 0, truncated: false };
    const images = this.ids.map((id) => {
      const [folder, name] = id.includes("/") ? id.split("/") : [null, id];
      return { id, name: name!, folder };
    });
    return { dir, unavailable: false, images, refused: this.refused, truncated: false };
  }

  /** A distinct PNG data URL per id: the PNG signature followed by the id's bytes. */
  image(s: AppSettings, id: string): string {
    if (s.portraitLibraryDir === null) throw apiError("invalid_argument", "No portrait library folder is chosen.");
    if (this.unavailable) throw apiError("invalid_argument", "Your portrait library folder is unavailable. Connect it, or choose it again in Settings > Content.");
    if (!this.ids.includes(id)) throw apiError("invalid_argument", "That image is not in your portrait library.");
    return `data:image/png;base64,${btoa(PNG_SIGNATURE + id)}`;
  }
}
