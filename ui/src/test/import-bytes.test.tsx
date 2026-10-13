import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { MockApi } from "../api/mock";
import type { ImportStartRequest } from "../api/contract";
import { bytesToBase64, problemWith } from "../pages/import/readSources";
import { renderApp } from "./utils";

const fast = () => new MockApi({ aiChunkDelayMs: 2, importStepMs: 1, aiEnabled: true, rng: () => 0.5 });

describe("import file sources", () => {
  it("sends dropped/picked file contents as base64 bytes, not paths", async () => {
    const user = userEvent.setup();
    const api = fast();
    const seen: ImportStartRequest[] = [];
    const original = api.importStart.bind(api);
    api.importStart = (req) => { seen.push(req); return original(req); };
    renderApp("/import", { api });
    const png = new File([new Uint8Array([137, 80, 78, 71, 1, 2, 3])], "feat page.png", { type: "image/png" });
    await user.upload(await screen.findByLabelText("Choose files"), png);
    await user.click(screen.getByRole("button", { name: "Import" }));
    await waitFor(() => expect(seen).toHaveLength(1));
    const src = seen[0]!.sources[0]!;
    expect(src).toEqual({ kind: "bytes", name: "feat page.png", mime: "image/png", base64: bytesToBase64(new Uint8Array([137, 80, 78, 71, 1, 2, 3])) });
  });

  it("refuses oversized and unsupported files before reading them", async () => {
    expect(problemWith({ name: "huge.pdf", type: "application/pdf", size: 21 * 1024 * 1024 })).toMatch(/too large/);
    expect(problemWith({ name: "photo.png", type: "image/png", size: 12 * 1024 * 1024 })).toBeNull();
    expect(problemWith({ name: "movie.mp4", type: "video/mp4", size: 10 })).toMatch(/not supported/);
    const user = userEvent.setup();
    renderApp("/import", { api: fast() });
    const big = new File(["x"], "big.pdf", { type: "application/pdf" });
    Object.defineProperty(big, "size", { value: 25 * 1024 * 1024 });
    await user.upload(await screen.findByLabelText("Choose files"), big);
    expect(await screen.findByText(/"big.pdf" is too large/)).toBeTruthy();
    expect((screen.getByRole("button", { name: "Import" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("sends typed notes next to the dropped files in one request", async () => {
    const user = userEvent.setup();
    const api = fast();
    const seen: ImportStartRequest[] = [];
    const original = api.importStart.bind(api);
    api.importStart = (req) => { seen.push(req); return original(req); };
    renderApp("/import", { api });
    const png = new File([new Uint8Array([137, 80, 78, 71])], "page.png", { type: "image/png" });
    await user.upload(await screen.findByLabelText("Choose files"), png);
    await user.type(await screen.findByLabelText("Notes about these files (optional)"), "this is a 2024 Fighter subclass");
    await user.click(screen.getByRole("button", { name: "Import" }));
    await waitFor(() => expect(seen).toHaveLength(1));
    expect(seen[0]!.sources.map((s) => s.kind)).toEqual(["bytes", "text"]);
    expect(seen[0]!.sources[1]).toEqual({ kind: "text", text: "this is a 2024 Fighter subclass" });
  });
});
