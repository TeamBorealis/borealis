import type { CharacterSnapshot } from "../types";
import { ABILITY_KEYS } from "../types";
import { ABILITY_NAMES } from "./content";
import { signed } from "./state";

const esc = (s: string): string => s.replace(/[&<>"]/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[ch]!);

/* Same shell as the Rust renderer (borealis-sheet html.rs): `body.sheet` with `section.page` pages, so the UI preview frame is exercised the same way. */
const CSS = `:root{--pw:8.5in;--ink:#17202a;--muted:#5d6b78;--accent:#1d6b66;--tint:#e6f2f0;--line:#c9d6d4;--bg:#e9eef0}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--ink);font:14px/1.4 system-ui,sans-serif}
.page{background:#fff;width:var(--pw);max-width:100%;margin:16px auto;padding:28px 30px;box-shadow:0 1px 6px rgba(0,0,0,.18);break-after:page}
header.top{display:flex;justify-content:space-between;align-items:flex-end;border-bottom:3px solid var(--accent);padding-bottom:8px;margin-bottom:14px}
h1{font:700 26px/1.1 Georgia,serif;margin:0}.sub{color:var(--muted)}
h2{font-size:.8em;text-transform:uppercase;letter-spacing:.09em;color:var(--accent);margin:16px 0 6px;border-bottom:1px solid var(--line)}
.stats{display:grid;grid-template-columns:repeat(auto-fit,minmax(110px,1fr));gap:8px;margin:8px 0}
.stat{border:1px solid var(--line);background:var(--tint);border-radius:8px;padding:6px 8px;text-align:center}.stat .v{font-size:1.5em;font-weight:700}
.cols{columns:2;padding-left:18px}table{width:100%;border-collapse:collapse}td{padding:2px 10px}`;

/** Mock sheet: a self-contained HTML document in our own layout. */
export function renderSheetHtml(s: CharacterSnapshot): string {
  const abil = ABILITY_KEYS.map((k) => `<div class="stat"><div>${ABILITY_NAMES[k].slice(0, 3).toUpperCase()}</div><div class="v">${s.abilities.final[k]}</div><div>${signed(s.abilities.modifier[k])}</div></div>`).join("");
  const skills = s.stats.skills.map((k) => `<li>${k.proficiency !== "none" ? "&#9679;" : "&#9675;"} ${esc(k.name)} <b>${signed(k.bonus)}</b></li>`).join("");
  const attacks = s.attacks.map((a) => `<tr><td>${esc(a.name)}</td><td>${esc(a.attackBonus)}</td><td>${esc(a.damage)}</td></tr>`).join("");
  const spells = s.spellcasting.classes.flatMap((c) => c.spells.filter((x) => x.prepared)).map((x) => `<li>${esc(x.name)} (${x.level || "cantrip"})</li>`).join("");
  const gear = s.inventory.items.filter((i) => i.listInInventory).map((i) => `<li>${i.quantity}&times; ${esc(i.displayName)}${i.equipped ? " (equipped)" : ""}</li>`).join("");
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>${esc(s.name)}</title><style>${CSS}</style></head><body class="sheet">
<section class="page"><header class="top"><div><h1>${esc(s.name)}</h1><div class="sub">Level ${s.level.total} &middot; ${s.ruleset === "dnd2024" ? "2024 rules" : "2014 rules"}</div></div></header>
<div class="stats">${abil}</div>
<p>AC <b>${s.stats.armorClass}</b> &middot; Initiative <b>${signed(s.stats.initiative)}</b> &middot; Speed <b>${s.stats.speed} ft</b> &middot; HP <b>${s.stats.hpMax}</b> &middot; Proficiency <b>${signed(s.stats.proficiencyBonus)}</b></p>
<h2>Skills</h2><ul class="cols">${skills}</ul><h2>Attacks</h2><table>${attacks || "<tr><td>None</td></tr>"}</table></section>
<section class="page">${spells ? `<h2>Spells</h2><ul class="cols">${spells}</ul>` : ""}<h2>Equipment</h2><ul class="cols">${gear || "<li>Nothing yet</li>"}</ul>
${s.details.backstory ? `<h2>Backstory</h2><p>${esc(s.details.backstory)}</p>` : ""}</section></body></html>`;
}
