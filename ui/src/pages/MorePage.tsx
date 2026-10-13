import { Link } from "react-router-dom";
import { Icon } from "../components";
import { ROUTES } from "../routes";
import { usePlatform } from "../state/platform";
import "./ai/panes.css";

/** Phone overflow destination (the fifth bottom-bar slot): a classic builder list of destinations. */
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
      </div>
    </div>
  );
}
