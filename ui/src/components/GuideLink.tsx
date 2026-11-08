import { LINKS, openLink } from "../lib/links";
import { useApi } from "../state/ApiContext";
import { ToolTip } from "./controls/Controls";
import { Icon } from "./Icon";

/** The website guides and the section each one explains (decision U18, B-234). */
export const GUIDES = {
  "create-a-character": { url: LINKS.guideCreateCharacter, title: "Create and level up a character" },
  "import-aurora-content": { url: LINKS.guideImportAurora, title: "Import Aurora content" },
  "import-5etools-data": { url: LINKS.guideImport5etools, title: "Import 5etools-format data" },
  "move-characters-between-devices": { url: LINKS.guideMoveCharacters, title: "Move characters between devices" },
  "print-character-sheet": { url: LINKS.guidePrintSheet, title: "Print or save a character sheet" },
} as const;

export type GuideId = keyof typeof GUIDES;

/** "Guide: <title>", the button's tooltip and accessible name. */
export function guideLabel(guide: GuideId): string {
  return `Guide: ${GUIDES[guide].title}`;
}

/**
 * Small help button beside a section's heading: a click or tap opens the section's website guide in the system
 * browser (U18). The tooltip shows on hover and keyboard focus; phones have no hover, so the tap itself opens the
 * guide and the aria-label names it. It never takes a dialog's first focus (`data-no-autofocus`).
 */
export function GuideLink({ guide }: { guide: GuideId }) {
  const api = useApi();
  const label = guideLabel(guide);
  return (
    <ToolTip text={label} className="aur-guide">
      <button type="button" className="aur-guide-btn" aria-label={label} data-no-autofocus onClick={() => void openLink(api, GUIDES[guide].url)}>
        <Icon name="help" />
      </button>
    </ToolTip>
  );
}
