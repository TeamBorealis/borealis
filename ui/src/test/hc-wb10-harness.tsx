import { act, render } from "@testing-library/react";
import { type ReactNode, useState } from "react";
import { MemoryRouter } from "react-router";
import type { HomebrewPickerRow } from "../api/contract";
import { MockApi } from "../api/mock";
import { MOCK_BLOCK_CATALOG, MOCK_STAT_CATALOG } from "../api/mock/homebrew-catalog";
import { AppProviders } from "../AppProviders";
import { type CreatorEnv, CreatorEnvValue } from "../pages/homebrew/env";

// Homebrew WB-10 test harness: one creator control with the app's providers and a creator environment holding the real
// block and statistic catalogs (the mock serves the ones `borealis-authoring` writes).

export const mockApi = (): MockApi => new MockApi({ aiChunkDelayMs: 2, rng: () => 0.5, creatorEnabled: true });

const row = (id: string, name: string): HomebrewPickerRow => ({ id, name, type: "class", summary: "", prerequisites: null, sourceName: "Sample Reference 5.1", edition: null, draft: false });
export const CLASSES: HomebrewPickerRow[] = [row("fighter", "Fighter"), row("paladin", "Paladin"), row("wizard", "Wizard")];

/** Renders, then lets the providers' first loads (settings) answer, so nothing updates after the test. */
export async function renderControl(ui: ReactNode, opts: { api?: MockApi; env?: Partial<CreatorEnv> } = {}) {
  const api = opts.api ?? mockApi();
  const env: Partial<CreatorEnv> = { blocks: MOCK_BLOCK_CATALOG, stats: MOCK_STAT_CATALOG, classes: CLASSES, ...opts.env };
  const utils = render(
    <AppProviders api={api} restore={false}>
      <MemoryRouter>
        <CreatorEnvValue value={env}>{ui}</CreatorEnvValue>
      </MemoryRouter>
    </AppProviders>,
  );
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  return { api, ...utils };
}

/** A controlled control: holds its value, reports every change to `onValue`. */
export function Stateful<V>({ initial, onValue, children }: { initial: V; onValue?: (v: V) => void; children: (value: V, set: (v: V) => void) => ReactNode }) {
  const [value, setValue] = useState(initial);
  return <>{children(value, (v) => { setValue(v); onValue?.(v); })}</>;
}
