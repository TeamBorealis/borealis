import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { CharacterSnapshot, ExportResult } from "../api/contract";
import { displayPath } from "../lib/displayPath";
import { renderApp, seededId } from "./utils";

const HOME_FILE = "/Users/example/Library/Application Support/studio.borealis/characters/Seraphine Vale.json";

describe("displayPath", () => {
  it("abbreviates macOS, Linux and Windows home folders to ~", () => {
    expect(displayPath(HOME_FILE)).toBe("~/Library/Application Support/studio.borealis/characters/Seraphine Vale.json");
    expect(displayPath("/home/example/Documents/Table Night/Seraphine Vale.pdf")).toBe("~/Documents/Table Night/Seraphine Vale.pdf");
    expect(displayPath("/var/home/example/x.pdf")).toBe("~/x.pdf");
    expect(displayPath("/root/x.pdf")).toBe("~/x.pdf");
    expect(displayPath("C:\\Users\\Example\\Documents\\Seraphine.pdf")).toBe("~\\Documents\\Seraphine.pdf");
    expect(displayPath("c:/users/example/x.pdf")).toBe("~/x.pdf");
    expect(displayPath("/Users/example")).toBe("~");
  });

  it("leaves paths outside a home folder unchanged", () => {
    expect(displayPath("/Volumes/USB/Seraphine.pdf")).toBe("/Volumes/USB/Seraphine.pdf");
    expect(displayPath("/Users/Shared/Seraphine.pdf")).toBe("/Users/Shared/Seraphine.pdf");
    expect(displayPath("/Usersexample/x.pdf")).toBe("/Usersexample/x.pdf");
    expect(displayPath("Seraphine.pdf")).toBe("Seraphine.pdf");
    expect(displayPath("")).toBe("");
  });
});

/** MockApi that writes like the desktop backend: absolute paths under the user's home folder. */
class DesktopPathsApi extends MockApi {
  override async exportSheet(args: Parameters<MockApi["exportSheet"]>[0]): Promise<ExportResult> {
    const r = await super.exportSheet(args);
    return { ...r, path: "/Users/example/Documents/Table Night/Seraphine Vale.pdf", dataBase64: null };
  }

  override async getSnapshot(args: Parameters<MockApi["getSnapshot"]>[0]): Promise<CharacterSnapshot> {
    return { ...(await super.getSnapshot(args)), displayPath: HOME_FILE };
  }

  override async loadCharacter(args: Parameters<MockApi["loadCharacter"]>[0]): Promise<CharacterSnapshot> {
    return { ...(await super.loadCharacter(args)), displayPath: HOME_FILE };
  }
}

describe("save paths on screen never show the home folder", () => {
  it("the character sheet export toast shows ~/...", async () => {
    const user = userEvent.setup();
    const api = new DesktopPathsApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
    const id = await seededId(api, "Seraphine");
    renderApp("/sheet", { api, openCharacterId: id });
    await user.click(await screen.findByRole("button", { name: /Save Character Sheet/ }));
    expect(await screen.findByText("Saved to ~/Documents/Table Night/Seraphine Vale.pdf")).toBeTruthy();
    expect(screen.queryByText(/\/Users\/example/)).toBeNull();
  });

  it("the status bar file field shows ~/... for a loaded character file", async () => {
    const api = new DesktopPathsApi({ aiChunkDelayMs: 1, rng: () => 0.5 });
    const id = await seededId(api, "Seraphine");
    renderApp("/builder", { api, openCharacterId: id });
    const bar = await screen.findByRole("contentinfo", { name: "Status" });
    await waitFor(() => expect(within(bar).getByText(/^~\/Library\/Application Support\//)).toBeTruthy());
    expect(within(bar).queryByText(/example/)).toBeNull();
  });
});
