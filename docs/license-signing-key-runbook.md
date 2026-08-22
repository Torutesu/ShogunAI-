# Licence signing key — runbook

**Status: the launch blocker for issue #97.** Every other piece of billing is in the tree; a
release build with no key in `EMBEDDED_PUBLIC_KEY_B64` verifies nothing and fails closed, so no
purchase can activate any Mac until this is done.

This is an owner task. The signing key is a secret that must exist in exactly one place — the
licence API's environment — and nobody should paste it into a terminal that logs, a chat, or a
pull request.

## What the two values are

`node scripts/gen-license-keypair.mjs` prints one line each:

| Value | Secret? | Where it goes |
|---|---|---|
| `LICENSE_SIGNING_KEY` | **Yes** | Licence API env only (`apps/website`). Never in the repo, never in the desktop app, never in a client bundle. |
| `SHOGUN_LICENSE_PUBKEY` | No | Three places, listed below. It is a public key; committing it is the point. |

The public key is not sensitive, but it *is* load-bearing: the whole plan gate rests on builds
verifying against the key the API signs with. Getting the two out of step is the failure mode this
runbook exists to prevent.

## First issuance

Run these in order. Do not sign anything with the new key until step 4 has shipped.

1. **Generate the pair**, on a machine you trust, and keep the output where you can read it once:

   ```
   node scripts/gen-license-keypair.mjs
   ```

2. **`LICENSE_SIGNING_KEY` → the licence API.** Set it as an environment variable on the
   `apps/website` deployment. Nothing else ever reads it. `signingKeyConfigured()`
   (`apps/website/src/lib/license.ts`) is what the verify route checks before it will sign.

3. **`SHOGUN_LICENSE_PUBKEY` → the batch relay.** Set it as `LICENSE_PUBKEY_B64` on `apps/api`.
   The relay verifies the same tokens locally, so a mismatch here fails Batch and ASR mint while
   the desktop app still looks fine — which is the confusing version of this outage.

4. **`SHOGUN_LICENSE_PUBKEY` → the shipped build.** Paste it into `EMBEDDED_PUBLIC_KEY_B64` in
   `crates/shogun-license/src/lib.rs`, commit, and **ship that build** before the API starts
   signing with the new key.

5. **`SHOGUN_LICENSE_PUBKEY` → dev / CI / staging.** These read the env var of the same name.
   Note it is honoured **only in debug builds** (`public_key()` is `#[cfg(debug_assertions)]` on
   the env branch) — a release build that trusted the environment would let anyone swap in their
   own keypair and mint themselves a Pro plan.

## Verifying it worked

Do all three. Each covers a different half of the system.

- **Purchase → activation.** In Stripe test mode, buy from the app. The Mac should activate by
  itself within a minute of payment (the claim poll runs every 5s for 15 minutes) — you should
  never have to touch the licence key field. If it stays on "Waiting for your purchase", check
  the webhook delivered and that step 2 is set.
- **Relay and ASR.** With the licence active, run something on the Batch lane and start a meeting.
  Both present the licence token as a bearer (`license_client::cached_license_token`); if step 3
  is wrong, these fail while the settings panel still shows a healthy plan.
- **Offline grace.** Cut the network. The app should keep working and start counting down —
  amber from day 7, cut off after 14 (FR-BIL-09). This proves the build is verifying the cached
  token rather than trusting it.

## Rotation

**Ship the build carrying the new public key first. Switch the signing key second.** The reverse
order drops every installed Mac into its offline-grace window on the next verification, and they
come back only when they update.

A rotation is therefore two releases, not one:

1. Ship a build whose `EMBEDDED_PUBLIC_KEY_B64` is the **new** key. Wait for adoption — anyone
   still on the old build starts failing the moment step 2 lands.
2. Switch `LICENSE_SIGNING_KEY` on the API and `LICENSE_PUBKEY_B64` on the relay to the new pair,
   together.

There is no dual-key verification path today. If rotation under load becomes a real requirement
(a leak, rather than hygiene), the change is to make `public_key()` return a set and have
`verify` accept any member — worth doing *before* you need it, not during.

## If the signing key leaks

The signed token is what a Mac trusts, and it is device-bound and short-lived (~24h), so a leaked
signing key lets an attacker mint a plan for a device id they know. It does not expose anyone's
licence key, memory, or capture content — those never reach the licence API at all
(FR-BIL-08, NFR-PRV-04).

Rotate per the section above, accepting that the two-release ordering is the cost of not having a
dual-key path. Revoking individual licences (`revoked_at` on `licenses`) does not help here: the
attacker is minting tokens, not using a licence.
