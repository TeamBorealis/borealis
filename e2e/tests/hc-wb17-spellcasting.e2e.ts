import { describe, test } from '@e2e-dev/web';
import { expect } from 'e2e';
import { bootToHome, expectEqual } from '../support/index.js';
import { pressKey } from '../support/flows-b.js';
import { statusMsg, waitFor, type Ctx } from '../support/flows-a.js';
import { noSkillChoice } from '../support/creator.js';

// Homebrew WB-17 (spec 4.6, 18.4 flow 3 on the web mock): the class's SPELLCASTING tab with the creator switched on
// (`?creator=1`, desktop 1355x847). Create a 2024 class, add spellcasting (a Spellcasting feature at level 1, made
// inside the class), choose a half caster (round up) and see its slot table (4 first- and 2 second-level slots at
// level 5, up to 5th-level slots), Wisdom, the Ranger's list; a custom table starts from those slots and takes a cell;
// save. The saved class opens again with its section. Deterministic: no model.

const NAME = '#hb-f-name-input';
const TABLE = 'table[aria-label="Spell slots and spells by level"]';

async function waitValue(ctx: Ctx, css: string, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, v }: { sel: string; v: string }) => (document.querySelector(sel) as HTMLInputElement | null)?.value === v, { sel: css, v: want }, `${css} = "${want}"`);
}

/** The slot cells of a level (read-only types), as shown. */
const slotsAt = (ctx: Ctx, level: number): Promise<string[]> =>
  ctx.browser.evaluate(({ sel, n }: { sel: string; n: number }) =>
    [...document.querySelectorAll(`${sel} tr[data-level="${n}"] .hb-slot-cell`)].map((c) => c.textContent ?? ''), { sel: TABLE, n: level });

async function waitSlots(ctx: Ctx, level: number, want: string): Promise<void> {
  await waitFor(ctx, ({ sel, n, w }: { sel: string; n: number; w: string }) =>
    [...document.querySelectorAll(`${sel} tr[data-level="${n}"] .hb-slot-cell`)].map((c) => c.textContent ?? '').join(',') === w,
  { sel: TABLE, n: level, w: want }, `slots at ${level} = ${want}`);
}

const autosaved = (ctx: Ctx): Promise<void> =>
  waitFor(ctx, (_: null) => /All changes kept/i.test(document.querySelector('.hb-autosave')?.textContent ?? ''), null, 'autosave');

describe('hc-wb17 homebrew spellcasting section (web mock)', { tags: ['homebrew'] }, () => {
  test('add spellcasting to a class, choose a caster type and see its slot table, a custom table, save', async (ctx) => {
    await bootToHome(ctx, '/?creator=1#/homebrew');
    await ctx.screen.getByRole('button', 'New pack').first().tap();
    await ctx.browser.locator('#hb-pack-name').fill('Ember Isles');
    await ctx.screen.getByRole('button', 'Create').tap();
    await expect(ctx.screen.getByRole('heading', 'Homebrew: Ember Isles')).toBeAttached();

    await ctx.screen.getByRole('button', /^New( element)?$/).first().tap();
    await expect(ctx.screen.getByRole('dialog', 'New element')).toBeVisible();
    await ctx.screen.getByRole('radio', /^Class A class/).tap();
    await ctx.browser.locator('#hb-new-name').fill('Warden');
    await ctx.screen.getByRole('button', 'Create').tap();
    await waitValue(ctx, NAME, 'Warden');
    await ctx.browser.locator('#hb-f-hitDie-input').selectOption('d10');
    await noSkillChoice(ctx);

    // SPELLCASTING: no spellcasting yet; add it (a full caster to start with)
    await ctx.screen.getByRole('tab', 'SPELLCASTING').tap();
    await expect(ctx.screen.getByText("This class doesn't cast spells.")).toBeVisible();
    await ctx.screen.getByRole('button', 'Add spellcasting').tap();
    await expect(ctx.screen.getByRole('table', 'Spell slots and spells by level')).toBeVisible();
    await waitSlots(ctx, 20, '4,3,3,3,3,2,2,1,1');

    // a half caster (round up): slots from level 1, 4 + 2 at level 5, up to 5th level
    await ctx.browser.locator('[data-field="casterType"] select').selectOption('Half caster (round up)');
    await waitSlots(ctx, 5, '4,2,-,-,-');
    expectEqual('level 1 slots', '2,-,-,-,-', (await slotsAt(ctx, 1)).join(','));
    expectEqual('level 20 slots', '4,3,3,3,2', (await slotsAt(ctx, 20)).join(','));
    await ctx.browser.locator('[data-field="ability"] select').selectOption('Wisdom');
    await ctx.browser.locator('[data-field="spellList"] input').fill('Ranger');
    await ctx.browser.locator(`${TABLE} input[aria-label="Prepared at level 5"]`).fill('6');

    // a custom table starts from the half caster's slots
    await ctx.browser.locator('[data-field="casterType"] select').selectOption('Custom table');
    await waitValue(ctx, `${TABLE} input[aria-label="2nd-level slots at level 5"]`, '2');
    await ctx.browser.locator(`${TABLE} input[aria-label="6th-level slots at level 20"]`).fill('1');
    await waitValue(ctx, `${TABLE} input[aria-label="6th-level slots at level 20"]`, '1');
    await autosaved(ctx);

    // The feature is on level 1 of the LEVELS grid.
    await ctx.screen.getByRole('tab', 'LEVELS').tap();
    await expect(ctx.screen.getByRole('button', 'Spellcasting')).toBeVisible();

    await pressKey(ctx, 'Control+s');
    await expect(ctx.screen.getByRole('button', 'Warden')).toBeVisible();
    expectEqual('status after save', "Saved 'Warden' to Ember Isles.", await statusMsg(ctx));

    // Reopened: the section is there with its custom table.
    await ctx.screen.getByRole('tab', 'SPELLCASTING').tap();
    await waitValue(ctx, `${TABLE} input[aria-label="6th-level slots at level 20"]`, '1');
    await waitValue(ctx, '[data-field="casterType"] select', 'custom');
  });
});
