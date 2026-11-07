import { useEffect } from "react";
import { useNavigate, useSearchParams } from "react-router";
import { useShell } from "../../components/shell/ShellContext";
import { ApplicationSettings } from "./ApplicationSettings";

/** Mounts the settings window while it is open (ShellContext); lives in the shell so it sits over the current screen. */
export function SettingsHost() {
  const { settingsTab, closeSettings } = useShell();
  return settingsTab !== null ? <ApplicationSettings initialTab={settingsTab} onClose={closeSettings} /> : null;
}

/** The old `/settings?tab=x` route: opens the window over the start screen and leaves the URL. */
export function SettingsRedirect() {
  const { openSettings } = useShell();
  const nav = useNavigate();
  const [params] = useSearchParams();
  const tab = params.get("tab") ?? undefined;
  useEffect(() => {
    openSettings(tab);
    nav("/characters", { replace: true });
  }, [openSettings, nav, tab]);
  return null;
}
