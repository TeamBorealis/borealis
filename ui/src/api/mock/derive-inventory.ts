import type { AbilityState, Coins, InventoryState } from "../types";
import type { MockCharacter } from "./state";

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

export function deriveInventory(c: MockCharacter, ab: AbilityState): InventoryState {
  const carried = c.items.filter((i) => i.storage === 0).reduce((s, i) => s + i.weight * i.quantity, 0);
  return {
    items: c.items,
    coins: c.coins,
    totalGp: totalCp(c.coins) / 100,
    carriedWeight: Math.round(carried * 10) / 10,
    capacity: ab.final.str * 15,
    attunement: { used: c.items.filter((i) => i.attuned).length, max: c.options.attunementLimit },
    storageNames: c.storageNames,
    warnings: c.warnings,
  };
}
