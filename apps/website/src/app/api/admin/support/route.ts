import { NextResponse } from 'next/server';

import { listSupportTickets, setSupportTicketStatus } from '@/db/support-queries';
import { adminRateLimited, isAdmin } from '@/lib/admin-auth';
import { HttpError, fail, readJsonObject } from '@/lib/http';
import { isSupportStatus } from '@/lib/support';
import { withTimeout } from '@/lib/timeout';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const DB_TIMEOUT_MS = 3_000;
const DEFAULT_LIMIT = 50;
const MAX_LIMIT = 200;

const NO_STORE = { 'X-Robots-Tag': 'noindex', 'Cache-Control': 'no-store' };

/**
 * GET /api/admin/support — triage listing for the CS窓口 (docs/support-runbook.md).
 * Query: ?status=open|triaged|resolved (optional), ?limit=1..200 (default 50).
 * Auth: x-admin-token header, same gate as /api/admin/stats. noindex.
 */
export async function GET(req: Request) {
  if (adminRateLimited(req)) return fail('rate_limited');
  if (!isAdmin(req)) return fail('forbidden');

  const url = new URL(req.url);
  const statusParam = url.searchParams.get('status');
  if (statusParam !== null && !isSupportStatus(statusParam)) return fail('bad_request');
  const limitParam = Number(url.searchParams.get('limit') ?? DEFAULT_LIMIT);
  const limit = Number.isInteger(limitParam) && limitParam >= 1 && limitParam <= MAX_LIMIT
    ? limitParam
    : DEFAULT_LIMIT;

  try {
    const tickets = await withTimeout(
      listSupportTickets(statusParam, limit),
      DB_TIMEOUT_MS,
      'support ticket list',
    );
    return NextResponse.json({ ok: true as const, tickets }, { headers: NO_STORE });
  } catch (e) {
    console.error('support ticket list error:', e);
    return fail('server_error');
  }
}

/**
 * PATCH /api/admin/support — move one ticket through the triage lifecycle.
 * Body: { id, status: open|triaged|resolved }. Auth as GET.
 */
export async function PATCH(req: Request) {
  if (adminRateLimited(req)) return fail('rate_limited');
  if (!isAdmin(req)) return fail('forbidden');

  let body: Record<string, unknown>;
  try {
    body = await readJsonObject(req);
  } catch (e) {
    if (e instanceof HttpError) return fail(e.code);
    return fail('bad_request');
  }
  // uuid shape, so a malformed id is a 400 instead of surfacing as a DB cast error.
  if (typeof body.id !== 'string' || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(body.id)) {
    return fail('bad_request');
  }
  if (!isSupportStatus(body.status)) return fail('bad_request');

  try {
    const updated = await withTimeout(
      setSupportTicketStatus(body.id, body.status),
      DB_TIMEOUT_MS,
      'support ticket status update',
    );
    if (!updated) return fail('not_found');
    return NextResponse.json({ ok: true as const }, { headers: NO_STORE });
  } catch (e) {
    console.error('support ticket status error:', e);
    return fail('server_error');
  }
}
