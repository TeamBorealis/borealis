import { type ReactNode, useState } from "react";
import { Icon } from "../../../components";
import { signed } from "../../../components/StatBlock";
import type { RollMode } from "../../../lib/dice";
import { type PressMenuItem, PressMenu } from "./PressMenu";
import { type DiceRequest, useRoller } from "./RollContext";
import { usePress } from "./usePress";

export interface PressButtonProps {
  /** What a tap does, for screen readers: "Roll Stealth check, +5". */
  label: string;
  /** The visible value: "+5", "1d8+3 slashing", "Cast". */
  children: ReactNode;
  /** The menu's name: "Roll Stealth check". */
  menuLabel: string;
  items: PressMenuItem[];
  /** The tap; `openMenu` opens the menu instead (a cast with no slot of its own level picks one). */
  onTap: (openMenu: () => void) => void;
  disabled?: boolean;
  icon?: boolean;
  /** Extra class of the roll button (column widths). */
  className?: string;
}

/**
 * A roll or cast button with its menu (GitHub #14): tap to roll, long press / right-click / the small arrow button
 * (mouse, keyboard) for Advantage, Normal and Disadvantage, or the other ways to roll or cast.
 */
export function PressButton({ label, children, menuLabel, items, onTap, disabled, icon = true, className = "" }: PressButtonProps) {
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);
  const press = usePress((el) => onTap(() => setAnchor(el)), setAnchor);
  return (
    <span className="pr-btns">
      <button type="button" className={`btn btn-sm pr-roll ${className}`.trim()} aria-label={label} disabled={disabled} {...press}>
        {icon && <Icon name="dice" />}<span className="pr-roll-value">{children}</span>
      </button>
      <button type="button" className="btn btn-sm pr-more" aria-label={`More: ${menuLabel}`} title="More ways" aria-haspopup="menu"
        aria-expanded={anchor !== null} disabled={disabled} onClick={(e) => setAnchor(e.currentTarget.previousElementSibling as HTMLElement)}>
        <Icon name="caret" />
      </button>
      {anchor && <PressMenu anchor={anchor} label={menuLabel} items={items} onClose={() => setAnchor(null)} />}
    </span>
  );
}

const MODES: { mode: RollMode; label: string }[] = [
  { mode: "advantage", label: "Advantage" },
  { mode: "normal", label: "Normal" },
  { mode: "disadvantage", label: "Disadvantage" },
];

export interface D20ButtonProps {
  /** "Stealth check", "Strength save", "Longsword attack". */
  what: string;
  modifier: number;
  /** A tap rolls with advantage when the snapshot says so (initiative advantage). */
  defaultMode?: RollMode;
  critRange?: number;
  damage?: DiceRequest | null;
  /** The visible text before the modifier ("Save"). */
  prefix?: string;
  /** Extra words for screen readers ("proficient"). */
  note?: string;
}

/** A d20 test: a tap rolls d20 + modifier; the menu rolls it with advantage or disadvantage. */
export function D20Button({ what, modifier, defaultMode = "normal", critRange, damage, prefix, note }: D20ButtonProps) {
  const roller = useRoller();
  const roll = (mode: RollMode) => roller.d20({ label: what, modifier, mode, critRange, damage });
  const tapNote = defaultMode === "normal" ? "" : `, ${defaultMode}`;
  return (
    <PressButton label={`Roll ${what}, ${signed(modifier)}${note ? `, ${note}` : ""}${tapNote}`} menuLabel={`Roll ${what}`}
      items={MODES.map((m) => ({ label: m.label, run: () => roll(m.mode) }))} onTap={() => roll(defaultMode)} className={prefix ? "pr-labelled" : ""}>
      {prefix ? `${prefix} ` : ""}{signed(modifier)}
    </PressButton>
  );
}
