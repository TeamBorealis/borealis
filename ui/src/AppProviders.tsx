import type { ReactNode } from "react";
import type { Api } from "./api/contract";
import { ConfirmProvider } from "./components/Confirm";
import { ImportNoticeProvider } from "./components/ImportNotice";
import { ToastProvider } from "./components/Toast";
import { ApiProvider } from "./state/ApiContext";
import { CharacterProvider } from "./state/CharacterContext";
import { SettingsProvider } from "./state/SettingsContext";

export function AppProviders({ api, children, restore = true }: { api: Api; children: ReactNode; restore?: boolean }) {
  return (
    <ApiProvider api={api}>
      <SettingsProvider>
        <ToastProvider>
          <ConfirmProvider>
            <ImportNoticeProvider>
              <CharacterProvider restore={restore}>{children}</CharacterProvider>
            </ImportNoticeProvider>
          </ConfirmProvider>
        </ToastProvider>
      </SettingsProvider>
    </ApiProvider>
  );
}
