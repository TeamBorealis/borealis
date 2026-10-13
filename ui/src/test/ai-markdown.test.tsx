import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CitedText } from "../pages/ai/CitedAnswer";

const SOURCES = [{ id: "ID_A", name: "Grappled", type: "Condition" }];
const show = (text: string, onOpen = vi.fn()) => render(<CitedText text={text} sources={SOURCES} onOpen={onOpen} />).container;

describe("Rules Q&A markdown", () => {
  it("renders headings, bold, italic and inline code as elements without raw markers", () => {
    const c = show("## Grapple\n\nA **grappled** creature has *speed* 0 and `0 ft`.");
    expect(c.querySelector("h3")?.textContent).toBe("Grapple");
    expect(c.querySelector("strong")?.textContent).toBe("grappled");
    expect(c.querySelector("em")?.textContent).toBe("speed");
    expect(c.querySelector("code")?.textContent).toBe("0 ft");
    expect(c.textContent).not.toMatch(/##|\*\*/);
  });

  it("renders bullet and numbered lists", () => {
    const c = show("- one\n- two\n\n1. first\n2. second");
    expect(c.querySelectorAll("ul li")).toHaveLength(2);
    expect(c.querySelectorAll("ol li")).toHaveLength(2);
  });

  it("formats partial streaming text instead of showing raw markers", () => {
    expect(show("A **gra").textContent).toBe("A gra");
    expect(show("A **gra").querySelector("strong")?.textContent).toBe("gra");
    expect(show("##").textContent).toBe("");
    expect(show("Speed [[ID_").textContent).toBe("Speed");
    expect(show("2 * 3 = 6").textContent).toBe("2 * 3 = 6");
  });

  it("renders script and img payloads as literal text, never as elements", () => {
    const c = show('<script>alert(1)</script> <img src=x onerror="alert(2)"> [click](javascript:alert(3))');
    expect(c.querySelector("script, img, a")).toBeNull();
    expect(c.textContent).toContain("<script>alert(1)</script>");
    expect(c.textContent).toContain('<img src=x onerror="alert(2)">');
    expect(c.textContent).not.toContain("javascript:");
  });

  it("keeps citations clickable inside formatted text", () => {
    const onOpen = vi.fn();
    const c = show("## Rule\n\n**Speed 0** [[ID_A]]\n\n- also [[ID_A]]", onOpen);
    const buttons = c.querySelectorAll("button.v5-cite");
    expect(buttons).toHaveLength(2);
    fireEvent.click(buttons[0]!);
    expect(onOpen).toHaveBeenCalledWith(SOURCES[0]);
    expect(c.textContent).not.toContain("[[");
  });
});
