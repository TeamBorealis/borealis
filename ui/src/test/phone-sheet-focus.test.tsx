import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { setPhoneForTests, setPhoneOsForTests } from "../lib/phone";
import { renderApp } from "./utils";

// iPhone: New Character focused its Name field while the sheet was still rising. The keyboard opened, the sheet
// scrolled to its bottom and the field being typed into was off screen (mobile e2e m1-m3 on the iPhone 18 Pro Max).
// On iOS and Android an opening dialog or sheet never focuses a text field (no keyboard until the user taps one);
// desktop keeps the autofocus.
afterEach(() => { setPhoneForTests(false); setPhoneOsForTests(null); });

const isTextEntry = (el: Element | null) => el instanceof HTMLTextAreaElement || (el instanceof HTMLInputElement && !["checkbox", "radio", "button", "submit", "reset", "range", "color", "file"].includes(el.type));

async function openNewCharacter(): Promise<HTMLElement> {
  const user = userEvent.setup();
  renderApp("/characters");
  await user.click((await screen.findAllByRole("button", { name: /new character/i }))[0]!);
  return screen.findByRole("dialog", { name: "New character" });
}

describe("opening a sheet on a phone", () => {
  it.each(["ios", "android"] as const)("%s: New Character does not focus the Name field, so no keyboard opens", async (os) => {
    setPhoneForTests(true);
    setPhoneOsForTests(os);
    const dialog = await openNewCharacter();
    expect(isTextEntry(document.activeElement)).toBe(false);
    expect(document.activeElement).not.toBe(screen.getByTestId("new-character-name"));
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(within(dialog).queryByRole("button", { name: "Close" })).not.toBe(document.activeElement);
  });

  it("a tablet (iOS, wide shell) does not focus the Name field either", async () => {
    setPhoneOsForTests("ios");
    await openNewCharacter();
    expect(isTextEntry(document.activeElement)).toBe(false);
  });

  it("desktop still puts the caret in the Name field", async () => {
    await openNewCharacter();
    expect(document.activeElement).toBe(screen.getByTestId("new-character-name"));
  });
});
