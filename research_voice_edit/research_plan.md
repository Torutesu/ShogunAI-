# Research plan: post-ASR voice transcript editor

## Main question
Which current model/provider shape should ShogunAI use for a low-latency post-ASR transcript editor, and what should the first implementation/bakeoff validate?

## Subtopics
1. ASR-native cleanup boundary: what Deepgram should handle before an edit model is called.
2. Hosted model candidates: current fast/low-cost APIs suitable for constrained transcript editing.
3. Safety and product contract: preservation checks, privacy boundary, latency budget, and fallback behavior.

## Synthesis
Use official provider documentation and the existing ShogunAI voice-edit specification to produce a ranked shortlist, a recommended provider-neutral interface, and a staged bakeoff plan. No implementation is part of this research pass.
