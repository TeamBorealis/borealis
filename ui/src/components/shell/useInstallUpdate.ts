import { useCallback } from "react";
import { useApi } from "../../state/ApiContext";
import { useCharacter } from "../../state/CharacterContext";
import { type AppInstallView, installAppUpdate, useAppInstallView } from "../../state/appUpdate";

/**
 * Install and restart for the update flyout and Settings: the open character's unsaved changes are saved first (the
 * restart would lose them), then the backend installs the update and restarts.
 */
export function useInstallUpdate(): { view: AppInstallView; run: () => void } {
  const api = useApi();
  const { snapshot } = useCharacter();
  const view = useAppInstallView();
  const run = useCallback(() => {
    void installAppUpdate(api, async () => {
      if (snapshot?.dirty) await api.saveCharacter({ characterId: snapshot.id });
    });
  }, [api, snapshot]);
  return { view, run };
}
