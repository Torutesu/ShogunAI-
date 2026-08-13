# SHOGUN API — meeting ASR token mint (spec)

Meeting ASR (2026-08-05 exception) needs a **company Deepgram key held server-side**. The desktop
app must not embed a long-lived key, so it asks this endpoint for a short-lived token per session.

Not implemented in this package yet — the batch relay (`src/`) is. This file is the contract the
device already speaks (`shogun_core::audio::asr::deepgram::EphemeralTokenAuth`).

## Endpoint

`POST $SHOGUN_ASR_TOKEN_URL`

**The URL must be `https://`** — the device refuses to construct the client otherwise, because
the request carries a bearer credential in each direction.

Request headers:

```
Authorization: Bearer <SHOGUN licence token>
Accept: application/json
```

The licence token is the same FR-BIL-08 `v1.<base64url(payload)>.<base64url(Ed25519 sig)>`
assertion the batch relay verifies — see `src/auth.ts` (`verifyLicense`), which is directly
reusable here. The device sends no body.

Response:

```json
{ "access_token": "<Deepgram JWT>", "expires_in": 60 }
```

## What the backend must do

1. **Verify the licence token** — signature against the licence API's Ed25519 public key
   (`LICENSE_PUBKEY_B64`), `exp` in the future, `plan` ∈ {standard, pro}, `status` ∈
   {active, trialing, past_due}. Reject with 401/402 exactly like the relay does. This is the
   step that stops anyone who learns the URL from minting tokens against the company key.
2. **Rate limit per licence** — the same reasoning as the relay's token bucket: one leaked token
   should not become a mint loop. A per-licence ceiling on concurrent or repeated mints is enough.
3. Call Deepgram `POST https://api.deepgram.com/v1/auth/grant` with the company key
   (`Authorization: Token …`, `ttl_seconds` ≤ 3600; prefer the smallest TTL a meeting needs).
4. Return the JWT. Never log the licence token, the Deepgram key, or the minted JWT.

The device then calls Deepgram listen with `Authorization: Bearer <jwt>` and always
`mip_opt_out=true` (enforced in the client constructor, not just on the wire).

Alternative design: proxy the listen/WS traffic entirely and never hand a Deepgram credential to
the client. Same authentication requirement at step 1.

## Device-side fallbacks

- **Keychain key** — the user's own Deepgram key, pasted in Settings (`KeychainKeyAuth`). Not the
  company key; no mint involved.
- **`SHOGUN_DEEPGRAM_API_KEY`** — debug builds only; hard-errors in release. See
  `shogun_core::audio::asr::deepgram`.
