import { afterEach, describe, expect, it, vi } from "vitest";
import { shareBase64File } from "./webShare";

const PDF_B64 = btoa("%PDF-1.4\n%%EOF");

function stubShare(share: (d: Record<string, unknown>) => Promise<void>, canShare = () => true) {
  const fn = vi.fn(share);
  Object.defineProperty(navigator, "share", { configurable: true, value: fn });
  Object.defineProperty(navigator, "canShare", { configurable: true, value: canShare });
  return fn;
}

afterEach(() => {
  Reflect.deleteProperty(navigator, "share");
  Reflect.deleteProperty(navigator, "canShare");
});

describe("shareBase64File (B-144 preview Share, A2 bundle share)", () => {
  it("shares only the file, with no title or text that iOS would save as a stray text.txt", async () => {
    // Arrange
    const share = stubShare(async () => {});
    // Act
    const result = await shareBase64File("Sable_Marrow.pdf", "application/pdf", PDF_B64);
    // Assert
    expect(result).toBe("shared");
    expect(share).toHaveBeenCalledTimes(1);
    const data = share.mock.calls[0]![0];
    expect(Object.keys(data)).toEqual(["files"]);
    const [file] = data.files as File[];
    expect(file!.name).toBe("Sable_Marrow.pdf");
    expect(file!.type).toBe("application/pdf");
    expect(file!.size).toBe("%PDF-1.4\n%%EOF".length);
  });

  it("answers cancelled when the user closes the share sheet", async () => {
    stubShare(async () => { throw Object.assign(new Error("closed"), { name: "AbortError" }); });
    expect(await shareBase64File("a.pdf", "application/pdf", PDF_B64)).toBe("cancelled");
  });

  it("answers unsupported when the web view cannot share files", async () => {
    const share = stubShare(async () => {}, () => false);
    expect(await shareBase64File("a.pdf", "application/pdf", PDF_B64)).toBe("unsupported");
    expect(share).not.toHaveBeenCalled();
  });

  it("passes any other share error on", async () => {
    stubShare(async () => { throw Object.assign(new Error("denied"), { name: "NotAllowedError" }); });
    await expect(shareBase64File("a.pdf", "application/pdf", PDF_B64)).rejects.toThrow("denied");
  });
});
