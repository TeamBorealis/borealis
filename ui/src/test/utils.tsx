import { render, screen } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { App } from "../App";
import { AppProviders } from "../AppProviders";
import { MockApi } from "../api/mock";

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
      <MemoryRouter initialEntries={[route]} future={{ v7_startTransition: true, v7_relativeSplatPath: true }}>
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

/** Opens View > Borealis Settings and selects one of its tabs. */
export async function openBorealisSettings(user: UserEvent, tab: string): Promise<void> {
  await user.click(await screen.findByRole("menuitem", { name: "View" }));
  await user.click(await screen.findByRole("menuitem", { name: "Borealis Settings" }));
  await user.click(await screen.findByRole("tab", { name: tab }));
}
