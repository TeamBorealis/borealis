import { Link } from "react-router";
import { Icon } from "../components";
import { ROUTES } from "../routes";
import { usePlatform } from "../state/platform";
import { openExportBundle, openReceiveFromDevice, openSendToDevice } from "./transfer/openTransfer";
import "./ai/panes.css";

/** Phone overflow destination (the fifth bottom-bar slot): a classic builder list of destinations, then Send / Receive. */
export function MorePage() {
  const { aiEnabled } = usePlatform();
  return (
    <div className="v5-page">
      <div className="v5-list-pane">
        <h1 className="sr-only">More</h1>
        <ul className="v5-list" aria-label="More destinations">
          {ROUTES.filter((r) => r.secondary && (!r.ai || aiEnabled)).map((r) => (
            <li key={r.path}>
              <Link className="v5-row" to={r.path} style={{ textDecoration: "none" }}><Icon name={r.icon} /><span className="grow">{r.label}</span><Icon name="chevron" /></Link>
            </li>
          ))}
        </ul>
        <ul className="v5-list" aria-label="Your other devices" style={{ marginTop: 10 }}>
          <li><button type="button" className="v5-row" onClick={() => openSendToDevice()}><Icon name="send" /><span className="grow">Send to device</span><Icon name="chevron" /></button></li>
          <li><button type="button" className="v5-row" onClick={() => openReceiveFromDevice()}><Icon name="download" /><span className="grow">Receive from device</span><Icon name="chevron" /></button></li>
          <li><button type="button" className="v5-row" onClick={() => openExportBundle()}><Icon name="upload" /><span className="grow">Share as a file</span><Icon name="chevron" /></button></li>
        </ul>
      </div>
    </div>
  );
}
