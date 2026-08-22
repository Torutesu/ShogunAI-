import { desc, eq } from 'drizzle-orm';

import { db } from './index';
import { supportTickets, type SupportTicket } from './schema';
import type { SupportReport, SupportSource, SupportStatus } from '@/lib/support';

/**
 * Support-ticket data layer (CS / bug-report intake). Plain inserts and reads — no upserts,
 * because every report is its own row; deduping is a triage decision, not a storage one.
 */

/** Insert one report; returns the new ticket id. */
export async function createSupportTicket(
  report: SupportReport,
  source: SupportSource,
  ipHash: string | null,
): Promise<string> {
  const rows = await db
    .insert(supportTickets)
    .values({
      source,
      category: report.category,
      message: report.message,
      email: report.email,
      appVersion: report.appVersion,
      osVersion: report.osVersion,
      plan: report.plan,
      ipHash,
    })
    .returning({ id: supportTickets.id });
  const id = rows[0]?.id;
  if (!id) throw new Error('support ticket insert returned no id');
  return id;
}

/** Newest-first listing for the admin surface, optionally filtered by status. */
export async function listSupportTickets(
  status: SupportStatus | null,
  limit: number,
): Promise<SupportTicket[]> {
  const base = db.select().from(supportTickets);
  const filtered = status ? base.where(eq(supportTickets.status, status)) : base;
  return filtered.orderBy(desc(supportTickets.createdAt)).limit(limit);
}

/** Move a ticket through the triage lifecycle. Returns false when the id is unknown. */
export async function setSupportTicketStatus(
  id: string,
  status: SupportStatus,
): Promise<boolean> {
  const rows = await db
    .update(supportTickets)
    .set({ status })
    .where(eq(supportTickets.id, id))
    .returning({ id: supportTickets.id });
  return rows.length > 0;
}
