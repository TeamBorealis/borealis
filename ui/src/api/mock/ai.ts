import type { AiEvent, AiMode, AiStatus, AiTaskRequest, AiTypedResult, ValidationReport } from "../types";
import { apiError } from "./errors";

/** Mock Rules Q&A citation: an element the mock content store has, so the chip opens a real entry. */
const CITED_ID = "sentinel";
const RULES_QA_ANSWER = `Short answer: yes. A creature you hit with an opportunity attack has its speed reduced to 0 for the rest of the turn, much like a grappled creature [[${CITED_ID}]]. (Mock answer based on the sample rules reference.)`;

const slug = (s: string): string => s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 40) || "creation";

function replyFor(req: AiTaskRequest): string {
  const p = req.prompt.trim();
  switch (req.mode) {
    case "rules_qa":
      return RULES_QA_ANSWER; // the real backend streams the validated answer, markers included
    case "build_advisor":
      return "Build advice:\n1. Raise your primary ability to 16+ before taking a feat.\n2. Your next level grants an ability score improvement; Tough or Alert are solid picks.\n3. Consider a defensive subclass if you often hold the front line.";
    case "explain":
      return "Your Armor Class is 10 plus your Dexterity modifier, plus any armor and shield bonuses. (Mock explanation from the sample trace.)";
    case "npc":
    case "backstory":
      return `Backstory draft:\n\nRaised at the edge of a quiet border town, your character learned early that kindness and caution are both survival skills. A debt of honor, never fully explained, set them on the road. ${p ? `Seed idea: ${p}.` : ""}`;
    default:
      return `Happy to help with that. You asked: "${p}". Here is a concise take you can refine further.`;
  }
}

function homebrewFor(prompt: string): Record<string, unknown> {
  const lower = prompt.toLowerCase();
  const type = lower.includes("feat") ? "feat" : lower.includes("spell") ? "spell" : "magic-item";
  const name = type === "feat" ? "Stoneward Reflexes" : type === "spell" ? "Ember Lattice" : "Lantern of Quiet Hours";
  return {
    id: `homebrew-${slug(name)}`, type, name, source: "sample-homebrew", rulesets: ["dnd2014", "dnd2024"],
    description: `<p>${name}: a custom ${type} generated from "${prompt.slice(0, 80)}".</p>`,
    ...(type === "spell" ? { level: 2, school: "Evocation" } : {}), ...(type === "magic-item" ? { rarity: "uncommon", attunement: true } : {}),
  };
}

export function validateHomebrew(json: Record<string, unknown>): ValidationReport {
  const errors: string[] = [];
  const warnings: string[] = [];
  for (const key of ["id", "type", "name", "source"]) if (typeof json[key] !== "string" || !(json[key] as string).trim()) errors.push(`Missing required string field "${key}".`);
  if (typeof json.id === "string" && !/^[a-z0-9][a-z0-9-]*$/.test(json.id)) errors.push('"id" must be lowercase letters, digits and dashes.');
  if (typeof json.description !== "string") warnings.push('No "description" provided.');
  if (json.type === "spell" && typeof json.level !== "number") errors.push('Spells need a numeric "level".');
  return { valid: errors.length === 0, errors, warnings };
}

function typedResultFor(req: AiTaskRequest): AiTypedResult | null {
  switch (req.mode) {
    case "rules_qa":
      return {
        kind: "rules_qa",
        data: {
          answerMarkdown: RULES_QA_ANSWER,
          citations: [CITED_ID],
          sources: [{ id: CITED_ID, name: "Sentinel", type: "feat" }],
          confidence: "high", notCovered: false, strippedCitations: [], uncitedClaims: [], unverified: false,
        },
      };
    case "explain":
      return { kind: "explain", data: { stat: req.params?.stat ?? req.prompt, source: "template", text: "Armor Class is 10 plus Dexterity modifier." } };
    default:
      return null;
  }
}

export interface MockAiOptions {
  chunkDelayMs: number;
  available: boolean;
}

/** Streams canned responses as `AiEvent`s, honouring cancellation. */
export class MockAi {
  private seq = 0;
  private cancelled = new Set<string>();
  constructor(private emit: (e: AiEvent) => void, private opts: MockAiOptions) {}

  status(): AiStatus {
    return this.opts.available
      ? { availability: "ready", provider: "claude", version: "mock", host: "local", detail: "Mock provider (no real model is called)." }
      : { availability: "unavailable", provider: "none", version: null, host: null, detail: "No claude or codex CLI was found on PATH." };
  }

  start(req: AiTaskRequest): string {
    if (!this.opts.available) throw apiError("ai_unavailable", "AI is unavailable.");
    const taskId = `task-${++this.seq}`;
    const text = replyFor(req);
    const words = text.split(/(\s+)/);
    const chunks: string[] = [];
    for (let i = 0; i < words.length; i += 4) chunks.push(words.slice(i, i + 4).join(""));
    let i = -1;
    const step = () => {
      if (this.cancelled.has(taskId)) {
        this.emit({ taskId, kind: "done", cancelled: true });
        return;
      }
      i += 1;
      if (i < chunks.length) {
        this.emit({ taskId, kind: "delta", text: chunks[i]! });
        setTimeout(step, this.opts.chunkDelayMs);
      } else {
        const typed = typedResultFor(req);
        if (typed) this.emit({ taskId, kind: "result", result: typed });
        if (req.mode === "homebrew") this.emit({ taskId, kind: "result", result: { kind: "homebrew", json: homebrewFor(req.prompt) } });
        this.emit({ taskId, kind: "done", cancelled: false });
      }
    };
    setTimeout(() => {
      this.emit({ taskId, kind: "started", mode: req.mode as AiMode });
      step();
    }, 0);
    return taskId;
  }

  cancel(taskId: string): void {
    this.cancelled.add(taskId);
  }
}
