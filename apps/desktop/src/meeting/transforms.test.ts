// The transcript transforms are what every meeting surface leans on (#122 memoizes them; this
// pins what they compute). Pure module — no Tauri, no React — so these run without the webview.
import { describe, expect, it } from "vitest";

import {
  buildTimeline,
  clock,
  groupTurns,
  minutesHasContent,
  translationPatchKey,
  usableTranslation,
  type TranscriptLine,
} from "./transforms";

function line(ts: number, speaker: string | null, text: string, translation?: string | null): TranscriptLine {
  return { ts, speaker, text, translation };
}

describe("clock", () => {
  it("renders mm:ss and clamps negatives to zero", () => {
    expect(clock(0)).toBe("0:00");
    expect(clock(61_000)).toBe("1:01");
    expect(clock(-500)).toBe("0:00");
    expect(clock(600_000)).toBe("10:00");
  });
});

describe("groupTurns", () => {
  it("merges consecutive lines from one speaker into a turn", () => {
    const turns = groupTurns([
      line(1_000, "me", "hello"),
      line(2_000, "me", "again"),
      line(3_000, "other", "hi"),
    ]);
    expect(turns).toHaveLength(2);
    expect(turns[0].text).toBe("hello again");
    expect(turns[1].text).toBe("hi");
  });

  it("rebases timestamps on the first line so turns start at zero", () => {
    const turns = groupTurns([line(50_000, "me", "a"), line(60_000, "other", "b")]);
    expect(turns[0].ts).toBe(0);
    expect(turns[1].ts).toBe(10_000);
  });

  it("does not merge across a translation change — the caption pair must stay aligned", () => {
    const turns = groupTurns([
      line(1_000, "other", "konnichiwa", "hello"),
      line(2_000, "other", "genki?", "how are you?"),
    ]);
    expect(turns).toHaveLength(2);
  });

  it("returns empty for empty input", () => {
    expect(groupTurns([])).toEqual([]);
  });
});

describe("buildTimeline", () => {
  it("buckets turns and truncates long details at 200 chars", () => {
    const lines: TranscriptLine[] = [];
    for (let i = 0; i < 6; i++) {
      // alternate speakers so each line is its own turn
      lines.push(line(1_000 + i * 1_000, i % 2 ? "me" : "other", `word${i} ${"x".repeat(60)}`));
    }
    const steps = buildTimeline(lines);
    expect(steps.length).toBeGreaterThan(1); // 6 turns cannot fit one 4-turn bucket
    for (const s of steps) {
      expect(s.detail.length).toBeLessThanOrEqual(200);
    }
  });

  it("starts a new bucket after a 90s gap", () => {
    const steps = buildTimeline([
      line(0, "me", "start"),
      line(1_000, "other", "reply"),
      line(200_000, "me", "much later"),
    ]);
    expect(steps).toHaveLength(2);
    expect(steps[1].detail).toBe("much later");
  });
});

describe("usableTranslation", () => {
  it("passes real translations through trimmed", () => {
    expect(usableTranslation("  こんにちは  ")).toBe("こんにちは");
  });

  it("drops empty and null", () => {
    expect(usableTranslation("")).toBeNull();
    expect(usableTranslation("   ")).toBeNull();
    expect(usableTranslation(null)).toBeNull();
    expect(usableTranslation(undefined)).toBeNull();
  });

  it("drops LLM meta-chat so it is never painted as a subtitle", () => {
    expect(usableTranslation("Sure, here's the translation: hello")).toBeNull();
    expect(usableTranslation("I don't see any audio content to translate")).toBeNull();
    expect(usableTranslation("Could you please provide the spoken line?")).toBeNull();
  });
});

describe("minutesHasContent", () => {
  it("is false only when every section is empty", () => {
    expect(minutesHasContent({ summary: " ", decisions: [], next_actions: [] })).toBe(false);
    expect(minutesHasContent({ summary: "shipped", decisions: [], next_actions: [] })).toBe(true);
    expect(
      minutesHasContent({ summary: "", decisions: [], next_actions: [{ text: "x", owner: null }] }),
    ).toBe(true);
  });
});

describe("translationPatchKey", () => {
  it("distinguishes speakers at the same timestamp, and null from empty-ish names", () => {
    expect(translationPatchKey(5, "me")).not.toBe(translationPatchKey(5, "other"));
    expect(translationPatchKey(5, null)).toBe(translationPatchKey(5, undefined));
    expect(translationPatchKey(5, null)).not.toBe(translationPatchKey(6, null));
  });
});
