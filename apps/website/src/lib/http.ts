import { NextResponse } from 'next/server';

/**
 * Uniform error shape across every endpoint: { ok:false, error:"<code>" }.
 * See REFERRAL_ENGINE.md §5.
 */
export type ErrorCode =
  | 'bad_request'
  | 'forbidden'
  | 'not_found'
  | 'payload_too_large'
  | 'rate_limited'
  | 'server_error';

const STATUS: Record<ErrorCode, number> = {
  bad_request: 400,
  forbidden: 403,
  not_found: 404,
  payload_too_large: 413,
  rate_limited: 429,
  server_error: 500,
};

export function fail(error: ErrorCode, extra?: Record<string, unknown>) {
  return NextResponse.json({ ok: false, error, ...extra }, { status: STATUS[error] });
}

export function ok<T extends Record<string, unknown>>(data: T, init?: ResponseInit) {
  return NextResponse.json({ ok: true, ...data }, init);
}

/** Max body size for public POSTs (spec §6.5). */
export const MAX_BODY_BYTES = 8 * 1024;

export class HttpError extends Error {
  constructor(public code: ErrorCode) {
    super(code);
  }
}

/**
 * Read and parse a JSON body with a hard size cap. Throws HttpError with
 * 'payload_too_large' over the cap and 'bad_request' for non-object roots
 * or invalid JSON. Streams so we never buffer more than the cap.
 */
export async function readJsonObject(req: Request): Promise<Record<string, unknown>> {
  const declared = req.headers.get('content-length');
  if (declared && Number(declared) > MAX_BODY_BYTES) throw new HttpError('payload_too_large');

  const reader = req.body?.getReader();
  if (!reader) throw new HttpError('bad_request');

  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > MAX_BODY_BYTES) {
      await reader.cancel();
      throw new HttpError('payload_too_large');
    }
    chunks.push(value);
  }

  const text = Buffer.concat(chunks).toString('utf8').trim();
  if (!text) return {};

  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new HttpError('bad_request');
  }
  // Reject non-object roots (arrays, strings, numbers, null) — spec §6.5.
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    throw new HttpError('bad_request');
  }
  return parsed as Record<string, unknown>;
}
