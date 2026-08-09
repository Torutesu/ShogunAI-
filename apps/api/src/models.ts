/**
 * Server-side model choice (docs/batch-relay-design.md §4.4).
 *
 * The device names an intent; the relay names the model. Changing a row here changes unit cost
 * for every customer, which is exactly why the choice cannot live on the device.
 */
import type { ModelClass } from "./types.js";

export const MODEL_BY_CLASS: Record<ModelClass, string> = {
  // Per-event labelling over a whole night of events — small and fast on purpose.
  classify: "claude-haiku-4-5-20251001",
  // Per-meeting summarisation — same cost posture as classify.
  summarize: "claude-haiku-4-5-20251001",
  // One Morning Brief per user per day — a bigger model is affordable at that volume.
  brief: "claude-sonnet-4-5-20250929",
};

export const MAX_TOKENS_BY_CLASS: Record<ModelClass, number> = {
  classify: 1024,
  summarize: 2048,
  brief: 2048,
};
