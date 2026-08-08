# SHOGUN API (scaffold)

Meeting ASR (2026-08-05) needs a **company Deepgram key held server-side**. Desktop must not embed a long-lived key.

## Required endpoint (not implemented here yet)

`POST $SHOGUN_ASR_TOKEN_URL` → JSON:

```json
{ "access_token": "<Deepgram JWT>", "expires_in": 60 }
```

Backend should:

1. Authenticate the paid SHOGUN user / device
2. Call Deepgram `POST https://api.deepgram.com/v1/auth/grant` with the company key (`Authorization: Token …`, optional `ttl_seconds` ≤ 3600)
3. Return the JWT to the desktop

Desktop then calls Deepgram listen with `Authorization: Bearer <jwt>` and always `mip_opt_out=true`.

Alternative: proxy the listen/WS traffic entirely and never expose a Deepgram credential to the client.

Local debug (dev builds only): `SHOGUN_DEEPGRAM_API_KEY` — see `shogun_core::audio::asr::deepgram`.
