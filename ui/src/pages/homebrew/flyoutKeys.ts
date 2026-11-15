import type { KeyboardEvent } from "react";

/** Enter in a one-line field of a flyout's form makes it (the Create button sits in the flyout's footer, outside the form,
 * so the browser does not submit it; WB-26). */
export function enterCreates(create: () => void) {
  return (e: KeyboardEvent<HTMLFormElement>) => {
    const t = e.target as HTMLElement;
    if (e.key !== "Enter" || e.nativeEvent.isComposing || !(t instanceof HTMLInputElement) || t.type === "checkbox") return;
    e.preventDefault();
    create();
  };
}
