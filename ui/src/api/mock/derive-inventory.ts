import type { AbilityState, Coins, InventoryItem, InventoryState } from "../types";
import { FEATS } from "./content";
import type { MockCharacter } from "./state";
import { withAttunementFacts } from "./attunement";
import { withRolledCharges } from "./rolled-charges";

export const COIN_VALUE_CP: Record<keyof Coins, number> = { cp: 1, sp: 10, ep: 50, gp: 100, pp: 1000 };
export const COIN_ORDER: (keyof Coins)[] = ["pp", "gp", "ep", "sp", "cp"];

export const totalCp = (coins: Coins): number => COIN_ORDER.reduce((s, k) => s + coins[k] * COIN_VALUE_CP[k], 0);

/** Greedy breakdown of a cp amount, highest denomination first (ep skipped for readability). */
export function breakdown(cp: number, from: keyof Coins = "pp"): Coins {
  const out: Coins = { cp: 0, sp: 0, ep: 0, gp: 0, pp: 0 };
  let rest = cp;
  for (const k of COIN_ORDER.slice(COIN_ORDER.indexOf(from))) {
    if (k === "ep") continue;
    out[k] = Math.floor(rest / COIN_VALUE_CP[k]);
    rest -= out[k] * COIN_VALUE_CP[k];
  }
  return out;
}

/** Pay `cost` cp; break the smallest single higher coin that covers the shortfall. */
export function pay(coins: Coins, cost: number): Coins | null {
  if (totalCp(coins) < cost) return null;
  const next = { ...coins };
  let rest = cost;
  for (const k of COIN_ORDER) {
    const use = Math.min(next[k], Math.floor(rest / COIN_VALUE_CP[k]));
    next[k] -= use;
    rest -= use * COIN_VALUE_CP[k];
  }
  if (rest > 0) {
    const asc = [...COIN_ORDER].reverse();
    const coin = asc.find((k) => next[k] > 0 && COIN_VALUE_CP[k] > rest);
    if (!coin) return null;
    next[coin] -= 1;
    const change = breakdown(COIN_VALUE_CP[coin] - rest, asc[asc.indexOf(coin) - 1] ?? "cp");
    for (const k of COIN_ORDER) next[k] += change[k];
  }
  return next;
}

/**
 * U12: an equipped "Additional Feat, X" line whose feat X has a "Level N+" prerequisite above the character's level is
 * withheld (as the engine does for any grant whose target fails its requirements) unless the line overrides it.
 */
function withWithheldGrants(c: MockCharacter, i: InventoryItem): InventoryItem {
  const override = i.overrideRequirements ?? false;
  const feat = /^Additional Feat, (.+)$/.exec(i.name)?.[1];
  const needs = feat ? /^Level (\d+)\+$/.exec(FEATS.find((f) => f.name === feat)?.prerequisites ?? "")?.[1] : undefined;
  const withheld = !!i.equipped && !override && !!needs && c.levelOrder.length < Number(needs);
  return { ...i, overrideRequirements: override, withheldGrants: withheld ? [feat!] : [] };
}

export function deriveInventory(c: MockCharacter, ab: AbilityState): InventoryState {
  const carried = c.items.filter((i) => i.storage === 0).reduce((s, i) => s + i.weight * i.quantity, 0);
  return {
    items: c.items.map((i) => withWithheldGrants(c, withRolledCharges(c, withAttunementFacts(c, i)))),
    coins: c.coins,
    totalGp: totalCp(c.coins) / 100,
    carriedWeight: Math.round(carried * 10) / 10,
    capacity: ab.final.str * 15,
    attunement: { used: c.items.filter((i) => i.attuned).length, max: c.options.attunementLimit },
    storageNames: c.storageNames,
    warnings: c.warnings,
  };
}
