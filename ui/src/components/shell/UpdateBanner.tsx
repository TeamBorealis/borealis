import { useApi } from "../../state/ApiContext";
import { dismissAppUpdate, openDownload, useAppUpdateView } from "../../state/appUpdate";

/** Slim bar under the sub-tab band: "Borealis X.Y.Z is available", Download, Dismiss (remembered per version). */
export function UpdateBanner() {
  const api = useApi();
  const { status, showBanner } = useAppUpdateView();
  if (!showBanner || !status?.latest) return null;
  const latest = status.latest;
  return (
    <div className="aur-update" role="region" aria-label="Update available">
      <span className="update-msg">Borealis {latest} is available</span>
      <button type="button" className="update-go" onClick={() => void openDownload(api, status.url ?? undefined)}>Download</button>
      <button type="button" onClick={() => dismissAppUpdate(latest)}>Dismiss</button>
    </div>
  );
}
