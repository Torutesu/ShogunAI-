# Local Willow binary findings

- Installed app: `/Applications/Willow Voice.app`
- Bundle version: `2.3.15`
- Executable: universal Mach-O (`x86_64` + `arm64`)
- Confirmed embedded service URLs include `https://api.willowvoice.com/api/v2`, `https://middleware.willowvoice.com`, and `https://db.willowvoice.com`.
- Confirmed embedded feature/type strings include `TranscriptCleanupMonitor`, `OpenAIAutopilot`, `DictationEditMetricsRequest`, `DictationUploadRequest`, `DictationUploadResponse`, and `TranscriptEditPair`.
- No readable transcript-cleanup system prompt was found in the executable or bundled resources with ASCII string extraction.

## Interpretation

The binary clearly contains a server-backed dictation/transcript pipeline, but the prompt is probably assembled remotely or delivered through API/config data rather than embedded as a plain string. The local inspection confirms architecture and endpoint names, not the vendor's private prompt.
