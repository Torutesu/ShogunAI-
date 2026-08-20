import { defineCloudflareConfig } from '@opennextjs/cloudflare';

/**
 * OpenNext → Cloudflare Workers adapter config.
 * Defaults are fine for this app (SSR + Node.js runtime API routes).
 * Add an incremental cache (R2/KV) here later if we start using ISR.
 */
export default defineCloudflareConfig({});
