import { useEffect, useState } from "react";
import { Flyout } from "../../components/controls/Flyout";
import { Button } from "../../components";
import { useCharacter } from "../../state/CharacterContext";
import { HouseRules } from "./OptionsTab";
import { RulesTab } from "./RulesTab";

const OPEN_EVENT = "borealis:edition";

/** Opens the Borealis "Edition & compatibility" flyout (menu entry in the character-name menu). */
export function openEditionFlyout() {
  window.dispatchEvent(new Event(OPEN_EVENT));
}

/** Borealis-only flyout, styled like the classic builder's: ruleset edition, compatibility report, Convert to 2024, house rules. */
export function EditionFlyout() {
  const [open, setOpen] = useState(false);
  const { snapshot } = useCharacter();
  useEffect(() => {
    const on = () => setOpen(true);
    window.addEventListener(OPEN_EVENT, on);
    return () => window.removeEventListener(OPEN_EVENT, on);
  }, []);
  if (!open || !snapshot) return null;
  const close = () => setOpen(false);
  return (
    <Flyout title="Edition & compatibility" onClose={close} footer={<div className="v4-foot one"><Button onClick={close}>Close</Button></div>}>
      <div className="mg-flyout v4-ed" tabIndex={-1} data-autofocus><RulesTab /><HouseRules /></div>
    </Flyout>
  );
}
