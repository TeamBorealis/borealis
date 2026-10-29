import { useState } from "react";
import { Button } from "../../components";
import { Flyout } from "../../components/controls/Flyout";
import { useShell } from "../../components/shell/ShellContext";
import { useCharacter } from "../../state/CharacterContext";
import { OptionCards, optionValue, optionsPatch, withBuiltIns } from "./OptionsTab";

function OptionsBody({ onClose }: { onClose: () => void }) {
  const { snapshot, mutate } = useCharacter();
  const [draft, setDraft] = useState<Record<string, boolean>>({});
  if (!snapshot) return null;
  const o = snapshot.options;
  const value = (id: string) => draft[id] ?? optionValue(o, id);
  const ok = async () => {
    const patch = optionsPatch(o, draft);
    if (Object.keys(patch).length) await mutate((a, id) => a.setCharacterOptions({ characterId: id, options: patch }));
    onClose();
  };
  return (
    <Flyout title="Character options" onClose={onClose}
      footer={<div className="v4-foot"><Button onClick={() => void ok()}>OK</Button><Button onClick={onClose}>Cancel</Button></div>}>
      <div className="mg-flyout" tabIndex={-1} data-autofocus>
        <OptionCards defs={withBuiltIns(snapshot.availableOptions)} value={value} onToggle={(id, on) => setDraft((d) => ({ ...d, [id]: on }))} />
      </div>
    </Flyout>
  );
}

/** Classic builder's "Character options" flyout (F8): 400 px option cards, OK applies the changes, Cancel discards them. */
export function OptionsFlyout() {
  const { optionsOpen, setOptionsOpen } = useShell();
  const { snapshot } = useCharacter();
  if (!optionsOpen || !snapshot) return null;
  return <OptionsBody onClose={() => setOptionsOpen(false)} />;
}
