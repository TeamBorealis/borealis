import { Link } from "react-router";
import { Icon } from "../../components";
import { usePlatform } from "../../state/platform";

/** Entry point shown in the Content, Compendium and AI page headers. */
export function ImportLink() {
  const { aiEnabled } = usePlatform();
  if (!aiEnabled) return null;
  return <Link className="btn btn-primary" to="/import"><Icon name="upload" />Import</Link>;
}
