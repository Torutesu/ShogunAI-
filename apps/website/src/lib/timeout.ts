/**
 * A deadline for work that is allowed to be slow but not allowed to hang.
 *
 * `try/catch` cannot express this. A rejected promise is an error and is caught; a promise that
 * never settles is neither, and on Cloudflare Workers the runtime eventually kills the whole
 * request ("detected that your Worker's code had hung and would never generate a response"). The
 * caller's fallback never runs, and a route written to degrade gracefully returns nothing at all.
 *
 * Postgres reached over the `nodejs_compat` socket shim does exactly this in production, which is
 * why the database calls on the purchase path are wrapped rather than merely caught.
 *
 * The losing promise is not cancelled — there is no way to cancel one — so the query may still
 * complete later against a connection nobody is reading. That is acceptable for the idempotent
 * reads and upserts this guards, and is the reason it is not applied to writes whose completion
 * the caller must know about.
 */
export async function withTimeout<T>(work: Promise<T>, ms: number, label: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      work,
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms);
      }),
    ]);
  } finally {
    // Without this the pending timer keeps the isolate's event loop alive past the response.
    if (timer !== undefined) clearTimeout(timer);
  }
}
