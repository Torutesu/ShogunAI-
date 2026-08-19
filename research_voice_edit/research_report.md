# Post-ASR voice edit model: current planning synthesis

## Recommendation

Use a provider-neutral, server-owned text-edit endpoint. Keep Deepgram Nova-3 responsible for speech recognition and deterministic formatting; invoke one fast text model only for semantic cleanup, then validate and fall back to the Deepgram result.

For first production candidate, test Cerebras-hosted `gpt-oss-120b` alongside GPT-4.1 nano and Gemini 2.5 Flash-Lite. Cerebras is attractive when edit quality plus extremely low latency matters; use `reasoning_effort=low` so the model does not spend budget on unnecessary reasoning.

## Current bakeoff shortlist

- GPT-4.1 nano: recommended default candidate. OpenAI prices it at $0.10/1M input and $0.40/1M output, with no reasoning step.
- Gemini 2.5 Flash-Lite: recommended comparison candidate. Google prices paid text input at $0.10/1M and output at $0.40/1M.
- Cerebras `gpt-oss-120b`: recommended speed/quality candidate. Cerebras documents roughly 3,000 tokens/second and $0.25/1M input plus $0.69/1M output.
- Gemini 3.5 Flash-Lite: quality challenger only; output is priced at $2.50/1M.
- Claude Haiku 4.5: quality reference, not default cost target.

Avoid Groq's Llama 3.1 8B Instant for new work: Groq marks it deprecated as of August 16, 2026.

Pin dated revisions server-side; do not make the desktop choose models.

## Scope of the edit lane

Input: final ASR text, locale, coarse destination surface, and a small protected-term list.

Allowed: remove fillers, stutters, repeated phrases, and abandoned false starts; improve punctuation, capitalization, paragraphing, and destination-appropriate structure.

Forbidden: answering the dictated content, adding facts, using tools/retrieval, rewriting names/numbers/URLs/code, or receiving audio/context outside the explicit request.

## Evaluation

Build a 100-200 example corpus with raw ASR, expected edited text, protected tokens, and destination. Score preservation failures first, then edit usefulness, p50/p95 added latency, timeout/fallback rate, cost, and multilingual behavior. A candidate fails if it mutates a critical identifier or makes the transcript less faithful.

## Rollout shape

1. Ship Deepgram-native formatting and local deterministic cleanup first.
2. Run the three providers offline/shadow against the fixed corpus.
3. Add one managed endpoint with a 1.5 s hard budget, no user-path retries, and silent raw fallback.
4. Gate by opt-in/plan policy; record only content-free trace metadata.

Deepgram's current documentation confirms Smart Format covers punctuation, paragraphs, and common entities such as dates, currency, phones, emails, and URLs; filler-word behavior is separately configurable. This is why the LLM should focus on semantic cleanup rather than basic formatting.
