import { render, screen, within } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { App } from "../App";
import { AppProviders } from "../AppProviders";
import { MockApi } from "../api/mock";
import { isPhone } from "../lib/phone";

export interface Harness {
  api: MockApi;
}

/** Renders the whole app against a fresh MockApi (no latency, fast AI stream). */
export function renderApp(route: string, opts: { api?: MockApi; openCharacterId?: string; aiEnabled?: boolean; importNotice?: boolean } = {}) {
  const api = opts.api ?? new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, aiEnabled: opts.aiEnabled });
  // The content-import notice has its own tests (import-notice.test.tsx); other flows start with it switched off.
  if (!opts.importNotice) api.presetSettings({ showImportNotice: false });
  try {
    if (opts.openCharacterId) sessionStorage.setItem("lk.currentCharacter", opts.openCharacterId);
    else sessionStorage.removeItem("lk.currentCharacter");
  } catch { /* ignore */ }
  const utils = render(
    <AppProviders api={api}>
      <MemoryRouter initialEntries={[route]}>
        <App />
      </MemoryRouter>
    </AppProviders>,
  );
  return { api, ...utils };
}

export async function seededId(api: MockApi, namePrefix: string): Promise<string> {
  const list = await api.listCharacters();
  const found = list.find((c) => c.name.startsWith(namePrefix));
  if (!found) throw new Error(`seed character ${namePrefix} missing`);
  return found.id;
}

/** Opens View > Application Settings (the one settings window, GitHub #31) and selects one of its tabs. */
export async function openSettingsTab(user: UserEvent, tab: string): Promise<void> {
  await user.click(await screen.findByRole("menuitem", { name: "View" }));
  await user.click(await screen.findByRole("menuitem", { name: "Application Settings" }));
  await user.click(await screen.findByRole("tab", { name: tab }));
}

/**
 * Chooses `option` (its label) in the select named `name`, inside `scope`: the native select on desktop, and on phones the
 * select button and its sheet (U22, B-138). The sheet renders outside `scope`, so it is found on the whole screen.
 */
export async function chooseOption(user: UserEvent, name: string | RegExp, option: string, scope: HTMLElement = document.body): Promise<void> {
  if (!isPhone()) {
    await user.selectOptions(within(scope).getByRole("combobox", { name }), option);
    return;
  }
  await user.click(within(scope).getByRole("button", { name }));
  const sheet = await screen.findByRole("dialog", { name });
  await user.click(within(sheet).getByRole("option", { name: option }));
}

/** The label of the option the select named `name` shows: the native select's selected option, or the phone select button's text. */
export function shownOption(name: string | RegExp, scope: HTMLElement = document.body): string {
  if (!isPhone()) {
    const select = within(scope).getByRole("combobox", { name }) as HTMLSelectElement;
    return select.selectedOptions[0]?.textContent ?? "";
  }
  return within(scope).getByRole("button", { name }).querySelector(".ph-select-value")?.textContent ?? "";
}
