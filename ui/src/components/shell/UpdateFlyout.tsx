import { useEffect, useState } from "react";
import { Button } from "../Button";
import { Icon } from "../Icon";
import { Flyout } from "../controls/Flyout";
import { ToolTip } from "../controls/Controls";
import { useApi } from "../../state/ApiContext";
import { Markdown } from "../../pages/ai/markdown";
import { canInstall, dismissAppUpdate, installLabel, openDownload, useAppUpdateView } from "../../state/appUpdate";
import type { UpdateMethod } from "../../api/types";
import { useInstallUpdate } from "./useInstallUpdate";

const OPEN_EVENT = "borealis:update-flyout";
export const UPDATE_TIP = "Update Available";

/** Opens the UPDATE AVAILABLE flyout (the title-bar update button). */
export function openUpdateFlyout(): void {
  window.dispatchEvent(new Event(OPEN_EVENT));
}

/**
 * Title-bar command shown while a newer release is known and not dismissed: accent fill, a dropping double chevron,
 * tooltip "Update Available". `compact` is the phone title bar's icon button.
 */
export function UpdateCommand({ compact = false }: { compact?: boolean }) {
  const { showNotice } = useAppUpdateView();
  if (!showNotice) return null;
  const button = (
    <button type="button" className={`aur-update-cmd${compact ? " aur-iconbtn" : ""}`} aria-label={UPDATE_TIP} onClick={openUpdateFlyout}>
      <span className="aur-update-glyph"><Icon name="update" /></span>
    </button>
  );
  return compact ? button : <ToolTip text={UPDATE_TIP}>{button}</ToolTip>;
}

/** The flyout's last line: how this copy gets the new version. */
export function howToUpdate(method: UpdateMethod): string {
  if (method === "install") return "Install and restart downloads it, checks its signature, installs it and restarts Borealis. Your characters and content stay where they are.";
  if (method === "package-manager") return "This copy was installed as a system package (deb or rpm): update it the way you installed it, or download the new package from the Borealis website. Your characters and content stay where they are.";
  return "Download it from the Borealis website. Your characters and content stay where they are.";
}

/**
 * Classic builder's update flyout: a description panel with the release, DOWNLOAD (success) and CLOSE. A direct
 * Windows, macOS or AppImage install also gets INSTALL AND RESTART (#25, UP2), which then takes the success colour.
 */
export function UpdateFlyout() {
  const api = useApi();
  const { status } = useAppUpdateView();
  const install = useInstallUpdate();
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const on = () => setOpen(true);
    window.addEventListener(OPEN_EVENT, on);
    return () => window.removeEventListener(OPEN_EVENT, on);
  }, []);
  const latest = status?.latest;
  if (!open || !latest) return null;
  const close = () => setOpen(false);
  /** CLOSE dismisses this version (remembered across launches); Esc and the scrim only close the flyout. */
  const dismiss = () => { dismissAppUpdate(latest); close(); };
  const installable = canInstall(status);
  const busy = install.view.phase === "installing" || install.view.phase === "restarting";
  return (
    <Flyout title="Update Available" onClose={close} footer={
      <div className="v4-foot">
        {installable && <Button variant="success" disabled={busy} onClick={install.run}>{installLabel(install.view)}</Button>}
        <Button variant={installable ? "default" : "success"} onClick={() => void openDownload(api, status.url ?? undefined)}>Download</Button>
        <Button onClick={dismiss}>Close</Button>
      </div>
    }>
      <div className="upd-flyout" tabIndex={-1} data-autofocus>
        <section className="upd-desc" aria-label="Release">
          <h4>Borealis {latest}</h4>
          <p>Version {latest} of Borealis is available. You have version {status.current}.</p>
          {status.notes && <div className="upd-notes" data-testid="update-notes"><Markdown text={status.notes} renderText={(t) => t} /></div>}
          <p>{howToUpdate(status.updateMethod)}</p>
          {installable && install.view.phase === "failed" && install.view.error && <p className="error-text" role="alert">{install.view.error}</p>}
        </section>
      </div>
    </Flyout>
  );
}
