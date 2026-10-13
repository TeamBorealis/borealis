import "./content.css";
import { FiveToolsTab } from "./FiveToolsTab";
import { IndexesTab } from "./IndexesTab";
import { PacksTab } from "./PacksTab";

/** START > ADDITIONAL CONTENT: stacked text sections (CUSTOM CONTENT, INDEX FILES, 5ETOOLS IMPORT); no nested tab row. */
export function ContentPage() {
  return (
    <div className="v4-content">
      <PacksTab />
      <IndexesTab />
      <FiveToolsTab />
    </div>
  );
}
