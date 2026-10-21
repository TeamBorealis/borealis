import { useEffect, useState } from "react";
import { Button } from "../Button";
import { Icon } from "../Icon";
import { Flyout } from "../controls/Flyout";
import { ToolTip } from "../controls/Controls";
import { useApi } from "../../state/ApiContext";
import { Markdown } from "../../pages/ai/markdown";
import { dismissAppUpdate, openDownload, useAppUpdateView } from "../../state/appUpdate";

const OPEN_EVENT = "borealis:update-flyout";
export const UPDATE_TIP = "Update available";

/** Opens the UPDATE AVAILABLE flyout (the title-bar update button). */
export function openUpdateFlyout(): void {
  window.dispatchEvent(new Event(OPEN_EVENT));
}

/**
 * Title-bar command shown while a newer release is known and not dismissed: accent fill, a dropping double chevron,
 * tooltip "Update available". `compact` is the phone title bar's icon button.
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

/** Classic builder's update flyout: title block, a description panel with the release, DOWNLOAD (success) and CLOSE. */
export function UpdateFlyout() {
  const api = useApi();
  const { status } = useAppUpdateView();
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
  return (
    <Flyout title="Update available" onClose={close} footer={
      <div className="v4-foot">
        <Button variant="success" onClick={() => void openDownload(api, status.url ?? undefined)}>Download</Button>
        <Button onClick={dismiss}>Close</Button>
      </div>
    }>
      <div className="upd-flyout" tabIndex={-1} data-autofocus>
        <h3 className="upd-title">Borealis {latest}</h3>
        <p className="upd-sub">A new version is available</p>
        <section className="upd-desc" aria-label="Release">
          <h4>Borealis {latest}</h4>
          <p>Version {latest} of Borealis is available. You have version {status.current}.</p>
          {status.notes && <div className="upd-notes" data-testid="update-notes"><Markdown text={status.notes} renderText={(t) => t} /></div>}
          <p>Download it from the Borealis website. Your characters and content stay where they are.</p>
        </section>
      </div>
    </Flyout>
  );
}
