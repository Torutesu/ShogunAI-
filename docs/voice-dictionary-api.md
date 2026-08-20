# Voice dictionary API boundary

`voice_terms` are owned by the encrypted local SQLite database. The supported Rust seam is
`shogun_core::daemon::Db`:

- `list_voice_terms`
- `create_voice_term`
- `update_voice_term`
- `delete_voice_term`

Desktop Settings, MCP, CLI, and loopback REST call a shared typed CRUD contract. Each adapter
uses the same `NewVoiceTerm` deserialization and delegates to the `Db` methods above; it cannot
write arbitrary dictionary JSON. All non-desktop faces use the existing local Memory API token and
Pro/trial entitlement gates. REST remains bound to loopback.

Locales are validated on write as `language` or `language-REGION` (for example `en` or `en-US`)
and stored normalized. Matching precedence is deterministic: the longest alias wins; ties use
scope `bundle > surface > global`, then higher priority. Equal remaining candidates stay
ambiguous and are not rewritten.

## Shipped management faces

- Desktop: Tauri commands `list_voice_dictionary_terms`, `create_voice_dictionary_term`,
  `update_voice_dictionary_term`, and `delete_voice_dictionary_term`.
- MCP: `voice_dictionary.list`, `.create`, `.update`, `.delete`.
- CLI: `shogun voice-dictionary list`, `create <term-json>`, `update <id> <term-json>`, and
  `delete <id>`.
- REST: `GET/POST /v1/voice_dictionary/terms`, `POST /v1/voice_dictionary/terms/<id>`, and
  `POST /v1/voice_dictionary/terms/<id>/delete`.

Term values and aliases are never logged or traced. Validation failures return only a generic
request error from network adapters.
