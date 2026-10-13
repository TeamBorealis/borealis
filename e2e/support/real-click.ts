import type { BrowserLike, LocatorLike, ScreenLike } from './types.js';

interface HitReport { ok: boolean; top: string }

/**
 * Real-pointer click: resolve the element's centre, confirm with `elementFromPoint` that the
 * element (or a descendant) is the topmost thing there, then click that viewport point with the
 * real mouse (`screen.tapAt` = Playwright `page.mouse.click`). A scrim or z-order bug that
 * covers the control fails here with the covering element named, whereas a locator
 * `tap()` would just auto-wait/scroll and may hit the same bug silently.
 *
 * `css` identifies the element in the page for the hit test (the locator cannot cross into
 * `evaluate`); it must match the element the locator resolves to.
 */
export async function realClick(
  { browser, screen }: { browser: BrowserLike; screen: ScreenLike },
  locator: LocatorLike,
  css: string,
): Promise<void> {
  await locator.waitFor({ state: 'visible' });
  const box = await locator.boundingBox();
  if (!box) throw new Error(`realClick: no box for ${css}`);
  const point = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const hit = await browser.evaluate(({ css, x, y }): HitReport => {
    const top = document.elementFromPoint(x, y);
    const describe = (e: Element | null) =>
      e ? `${e.tagName.toLowerCase()}${e.id ? '#' + e.id : ''}${e.className && typeof e.className === 'string' ? '.' + e.className.trim().split(/\s+/).join('.') : ''}` : 'null';
    const targets = Array.from(document.querySelectorAll(css));
    const ok = !!top && targets.some((t) => t === top || t.contains(top));
    return { ok, top: describe(top) };
  }, { css, ...point });
  if (!hit.ok) throw new Error(`realClick: ${css} is covered at (${Math.round(point.x)}, ${Math.round(point.y)}) by ${hit.top}`);
  await screen.tapAt(point);
}
