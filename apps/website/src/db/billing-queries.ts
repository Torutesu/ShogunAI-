import { eq, sql } from 'drizzle-orm';

import { db } from './index';
import {
  billingCustomers,
  licenses,
  stripeEvents,
  subscriptions,
  type License,
  type Subscription,
} from './schema';
import type { SubscriptionRecord } from '@/lib/billing';
import { canonicalLicenseKey } from '@/lib/license';

/**
 * Billing data layer (issue #8). Every write is an upsert keyed on the Stripe id, because
 * webhooks arrive out of order and more than once — `customer.subscription.updated` routinely
 * beats `checkout.session.completed`. Ordering-independence is a requirement here, not a nicety.
 */

const toDate = (secs: number | null | undefined): Date | null =>
  typeof secs === 'number' && Number.isFinite(secs) ? new Date(secs * 1000) : null;

/** Record an event id; returns false when we have already processed it (idempotency guard). */
export async function claimStripeEvent(id: string, type: string): Promise<boolean> {
  const rows = await db
    .insert(stripeEvents)
    .values({ id, type })
    .onConflictDoNothing({ target: stripeEvents.id })
    .returning({ id: stripeEvents.id });
  return rows.length > 0;
}

/**
 * Undo a claim after the handler failed. Without this, a transient DB error would burn the
 * event id: Stripe's retry would see "already processed" and drop the update on the floor.
 */
export async function releaseStripeEvent(id: string): Promise<void> {
  await db.delete(stripeEvents).where(eq(stripeEvents.id, id));
}

export async function findCustomerByEmail(email: string) {
  const rows = await db
    .select()
    .from(billingCustomers)
    .where(eq(billingCustomers.email, email.trim().toLowerCase()))
    .limit(1);
  return rows[0] ?? null;
}

export async function linkCustomer(email: string, stripeCustomerId: string) {
  const normalized = email.trim().toLowerCase();
  await db
    .insert(billingCustomers)
    .values({ email: normalized, stripeCustomerId })
    .onConflictDoUpdate({
      target: billingCustomers.email,
      set: { stripeCustomerId },
    });
}

export async function upsertSubscription(rec: SubscriptionRecord): Promise<void> {
  const values = {
    stripeSubscriptionId: rec.stripeSubscriptionId,
    stripeCustomerId: rec.stripeCustomerId,
    stripePriceId: rec.stripePriceId,
    plan: rec.plan,
    interval: rec.interval,
    status: rec.status,
    currentPeriodStart: toDate(rec.currentPeriodStart),
    currentPeriodEnd: toDate(rec.currentPeriodEnd),
    cancelAt: toDate(rec.cancelAt),
    canceledAt: toDate(rec.canceledAt),
    cancelAtPeriodEnd: rec.cancelAtPeriodEnd,
    trialEnd: toDate(rec.trialEnd),
    updatedAt: new Date(),
  };
  await db
    .insert(subscriptions)
    .values(values)
    .onConflictDoUpdate({
      target: subscriptions.stripeSubscriptionId,
      set: { ...values },
    });
}

export async function findSubscription(stripeSubscriptionId: string): Promise<Subscription | null> {
  const rows = await db
    .select()
    .from(subscriptions)
    .where(eq(subscriptions.stripeSubscriptionId, stripeSubscriptionId))
    .limit(1);
  return rows[0] ?? null;
}

/**
 * Ensure a licence exists for a subscription and return it. The key is minted at most once per
 * subscription: on conflict we return the existing row, so a replayed webhook never rotates a key
 * out from under a Mac that is already using it.
 */
export async function ensureLicense(args: {
  licenseKey: string;
  stripeCustomerId: string;
  stripeSubscriptionId: string;
  email: string | null;
}): Promise<License> {
  await db
    .insert(licenses)
    .values({
      licenseKey: canonicalLicenseKey(args.licenseKey),
      stripeCustomerId: args.stripeCustomerId,
      stripeSubscriptionId: args.stripeSubscriptionId,
      email: args.email,
    })
    .onConflictDoNothing({ target: licenses.stripeSubscriptionId });

  const rows = await db
    .select()
    .from(licenses)
    .where(eq(licenses.stripeSubscriptionId, args.stripeSubscriptionId))
    .limit(1);
  const row = rows[0];
  if (!row) throw new Error('license upsert did not produce a row');
  return row;
}

export async function findLicenseByKey(rawKey: string): Promise<License | null> {
  const rows = await db
    .select()
    .from(licenses)
    .where(eq(licenses.licenseKey, canonicalLicenseKey(rawKey)))
    .limit(1);
  return rows[0] ?? null;
}

export async function findLicenseBySubscription(subId: string): Promise<License | null> {
  const rows = await db
    .select()
    .from(licenses)
    .where(eq(licenses.stripeSubscriptionId, subId))
    .limit(1);
  return rows[0] ?? null;
}

/**
 * Stamp a successful verification. `device_count` only moves when a *different* device id shows
 * up, which is all the seat-abuse signal we want — we deliberately do not keep a device list.
 */
export async function recordVerification(
  licenseId: string,
  deviceId: string,
  appVersion: string | null,
): Promise<void> {
  await db
    .update(licenses)
    .set({
      lastDeviceId: deviceId,
      lastAppVersion: appVersion,
      lastVerifiedAt: new Date(),
      deviceCount: sql`CASE
        WHEN ${licenses.lastDeviceId} IS NULL THEN 1
        WHEN ${licenses.lastDeviceId} = ${deviceId} THEN ${licenses.deviceCount}
        ELSE ${licenses.deviceCount} + 1 END`,
    })
    .where(eq(licenses.id, licenseId));
}
