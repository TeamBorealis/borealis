import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MockApi } from "../api/mock/MockApi";
import { ZIP_BUTTON } from "../pages/content/FiveToolsTab";
import { renderApp } from "./utils";

const mock = (extra: ConstructorParameters<typeof MockApi>[0] = {}) => new MockApi({ rng: () => 0.5, ...extra });
const chooseZip = (file: File) => fireEvent.change(screen.getByLabelText("Choose a 5etools .zip"), { target: { files: [file] } });

describe("content tab on phones: 5etools .zip, no dead buttons", () => {
  it("on mobile offers only the zip chooser, then scans and imports the unpacked folder", async () => {
    const api = mock({ mobile: true });
    const stage = vi.spyOn(api, "stage5etoolsZip");
    const pickPath = vi.spyOn(api, "pickPath");
    renderApp("/content", { api });
    await screen.findByRole("button", { name: ZIP_BUTTON });
    expect(screen.queryByRole("button", { name: "Scan folder…" })).toBeNull();
    // No folder buttons that would fail on a phone.
    expect(screen.queryByRole("button", { name: "Open user" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Open" })).toBeNull();
    expect(screen.queryByRole("button", { name: /^Restart/ })).toBeNull();
    // Android has no MIME type for .index; untyped files must stay choosable there.
    expect((screen.getByLabelText("Choose content files") as HTMLInputElement).accept).toContain("application/octet-stream");
    const input = screen.getByLabelText("Choose a 5etools .zip") as HTMLInputElement;
    expect(input.accept).toContain(".zip");
    expect(input.multiple).toBe(false);

    chooseZip(new File(["PK"], "data.zip"));
    const list = await screen.findByRole("list", { name: "Books sources" });
    expect(stage).toHaveBeenCalledWith({ file: { fileName: "data.zip", contentBase64: btoa("PK") } });
    expect(pickPath).not.toHaveBeenCalled();
    expect((screen.getByLabelText("Data folder .zip") as HTMLInputElement).value).toBe("data.zip");
    await userEvent.setup().click(within(list).getByRole("checkbox", { name: /Basic Rules/ }));
    await userEvent.setup().click(screen.getByRole("button", { name: /Import \d+ book\(s\)/ }));
    expect(await screen.findByRole("heading", { name: "Import report" })).toBeTruthy();
    expect((await api.listPacks()).some((p) => p.id.startsWith("pack-5et-"))).toBe(true);
  });

  it("shows the backend's refusal for a file that is not a zip", async () => {
    renderApp("/content", { api: mock({ mobile: true }) });
    await screen.findByRole("button", { name: ZIP_BUTTON });
    chooseZip(new File(["{}"], "races.json"));
    await waitFor(() => expect(screen.getByText("Choose a .zip of your 5etools data/ folder.")).toBeTruthy());
    expect(screen.queryByRole("list", { name: "Books sources" })).toBeNull();
  });

  it("desktop keeps the folder scan and also accepts the zip", async () => {
    renderApp("/content", { api: mock() });
    expect(await screen.findByRole("button", { name: "Scan folder…" })).toBeTruthy();
    expect(screen.getByRole("button", { name: ZIP_BUTTON })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open user" })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^Restart/ })).toBeTruthy();
    expect((screen.getByLabelText("Choose content files") as HTMLInputElement).accept).not.toContain("octet-stream");
    chooseZip(new File(["PK"], "data.zip"));
    expect(await screen.findByRole("list", { name: "Books sources" })).toBeTruthy();
  });
});
