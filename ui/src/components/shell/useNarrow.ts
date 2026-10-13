import { useEffect, useState } from "react";

/** Breakpoint below which the shell switches to the phone/tablet layout (hamburger + bottom bar). */
export const NARROW_QUERY = "(max-width: 899px)";

/** True on narrow viewports. jsdom (no matchMedia) is treated as desktop. */
export function useNarrow(): boolean {
  const get = () => typeof window !== "undefined" && typeof window.matchMedia === "function" && window.matchMedia(NARROW_QUERY).matches;
  const [narrow, setNarrow] = useState(get);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia(NARROW_QUERY);
    const on = () => setNarrow(mq.matches);
    mq.addEventListener("change", on);
    on();
    return () => mq.removeEventListener("change", on);
  }, []);
  return narrow;
}
