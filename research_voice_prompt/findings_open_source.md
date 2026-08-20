# Open-source prompt findings

- Flow describes a compact formatting pass after ASR, with Groq or Ollama, and raw-transcript fallback on formatter failure: https://github.com/jgvilchezc/flow
- Muninn exposes a short conservative developer-dictation base prompt: minimal corrections; preserve technical terms, commands, paths, flags, acronyms, and obvious errors; keep original wording when uncertain. Its sample config also uses strict output bounds: 512 max output tokens, 25% length delta, 60% token-change ratio, and at most 2 new words: https://github.com/bnomei/muninn
- Murmur describes conservative cleanup with deterministic vocabulary/corrections and explicitly avoids turning garbled or repetitive passages into invented text: https://github.com/paretoimproved/murmur
- A publicly shared MacWhisper prompt emphasizes output-only, minimal meaning-preserving edits, punctuation/capitalization, and spoken-punctuation conversion: https://gist.github.com/briansunter/432e1db8746d0146623b7e4c744d9a0c

## Interpretation

The strongest common pattern is not a long role prompt. It is a short conservative instruction plus deterministic vocabulary protection, strict output validation, and raw fallback.
