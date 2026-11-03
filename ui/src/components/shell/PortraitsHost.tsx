import { PortraitsFlyout } from "../../pages/manage/PortraitsFlyout";
import { useCharacter } from "../../state/CharacterContext";
import { useShell } from "./ShellContext";

/**
 * Mounts the portraits gallery while `portraitsOpen` is set and a character is open. It lives in the shell (not in the
 * wide profile block) so every way in opens it in both layouts: the profile, quick view, save and phone title-bar
 * portraits, View > Portraits Gallery and Ctrl+P.
 */
export function PortraitsHost() {
  const { portraitsOpen, setPortraitsOpen } = useShell();
  const { snapshot, mutate } = useCharacter();
  if (!portraitsOpen || !snapshot) return null;
  return (
    <PortraitsFlyout current={snapshot.portrait} onClose={() => setPortraitsOpen(false)}
      onSelect={(dataUrl) => void mutate((api, id) => api.setPortrait({ characterId: id, dataUrl }))} />
  );
}
