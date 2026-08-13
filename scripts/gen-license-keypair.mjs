#!/usr/bin/env node
/**
 * Generate the Ed25519 keypair that signs ShogunAI licence tokens (issue #8 / FR-BIL-08).
 *
 *   node scripts/gen-license-keypair.mjs
 *
 * Prints two values:
 *   LICENSE_SIGNING_KEY        — base64(PKCS#8 PEM). SECRET. Set it on the licence API only
 *                                (apps/website env). It never goes in the repo, in the desktop
 *                                app, or in a client bundle.
 *   SHOGUN_LICENSE_PUBKEY      — base64(raw 32-byte Ed25519 public key). NOT secret. Paste it
 *                                into `LICENSE_PUBKEY_B64` in
 *                                crates/shogun-agents/src/license.rs so shipped builds verify
 *                                against it; the same value also works as an env override for
 *                                dev and tests.
 *
 * Rotation: publish a build carrying the new public key BEFORE switching the signing key, or
 * every already-installed Mac fails verification and falls into the offline-grace window.
 */
import { generateKeyPairSync } from 'node:crypto';

const { privateKey, publicKey } = generateKeyPairSync('ed25519');

const pkcs8Pem = privateKey.export({ type: 'pkcs8', format: 'pem' });
// SPKI DER for Ed25519 is a fixed 12-byte header followed by the 32-byte raw key.
const rawPublic = publicKey.export({ type: 'spki', format: 'der' }).subarray(-32);

process.stdout.write(`LICENSE_SIGNING_KEY=${Buffer.from(pkcs8Pem).toString('base64')}\n`);
process.stdout.write(`SHOGUN_LICENSE_PUBKEY=${rawPublic.toString('base64')}\n`);
