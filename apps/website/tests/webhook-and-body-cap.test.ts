import assert from 'node:assert/strict';
import { test } from 'node:test';
import type Stripe from 'stripe';

import { toSubscriptionRecord, type StripeSubscriptionLike } from '../src/lib/billing.ts';
import { HttpError, MAX_BODY_BYTES } from '../src/lib/http.ts';
import { readSignupBody } from '../src/app/api/waitlist/signup/route.ts';
import { makeWebhookHandler } from '../src/app/api/stripe/webhook/route.ts';

const subscription = (status: string): StripeSubscriptionLike => ({
  id: 'sub_123',
  status,
  customer: 'cus_123',
  items: { data: [{ price: { id: 'price_unknown' }, current_period_end: 1_800_000_000 }] },
});

function subscriptionEvent(type: string, body: StripeSubscriptionLike): Stripe.Event {
  return {
    id: 'evt_123',
    type,
    data: { object: body },
  } as unknown as Stripe.Event;
}

function request() {
  return new Request('https://syogun.com/api/stripe/webhook', {
    method: 'POST',
    headers: { 'stripe-signature': 'sig' },
    body: '{}',
  });
}

test('webhook uses Stripe current state instead of a stale subscription event', async () => {
  const saved: ReturnType<typeof toSubscriptionRecord>[] = [];
  const stale = subscription('active');
  const current = subscription('canceled');
  const handler = makeWebhookHandler({
    stripeSecretKey: () => 'sk_test',
    webhookSecret: () => 'whsec_test',
    stripe: () =>
      ({
        webhooks: {
          constructEventAsync: async () =>
            subscriptionEvent('customer.subscription.updated', stale),
        },
        subscriptions: { retrieve: async () => current },
      }) as unknown as ReturnType<typeof import('../src/lib/stripe.ts').stripe>,
    claimStripeEvent: async () => true,
    upsertSubscription: async (record) => {
      saved.push(record);
    },
  });

  const response = await handler(request());
  assert.equal(response.status, 200);
  assert.equal(saved.length, 1);
  assert.equal(saved[0].status, 'canceled');
});

test('deleted subscription event persists terminal payload when Stripe re-read fails', async () => {
  const saved: ReturnType<typeof toSubscriptionRecord>[] = [];
  const deleted = subscription('canceled');
  const handler = makeWebhookHandler({
    stripeSecretKey: () => 'sk_test',
    webhookSecret: () => 'whsec_test',
    stripe: () =>
      ({
        webhooks: {
          constructEventAsync: async () =>
            subscriptionEvent('customer.subscription.deleted', deleted),
        },
        subscriptions: {
          retrieve: async () => {
            throw new Error('not found');
          },
        },
      }) as unknown as ReturnType<typeof import('../src/lib/stripe.ts').stripe>,
    claimStripeEvent: async () => true,
    upsertSubscription: async (record) => {
      saved.push(record);
    },
  });

  const response = await handler(request());
  assert.equal(response.status, 200);
  assert.equal(saved.length, 1);
  assert.equal(saved[0].status, 'canceled');
});

test('webhook releases its event claim when persisting the current state fails', async () => {
  const released: string[] = [];
  const handler = makeWebhookHandler({
    stripeSecretKey: () => 'sk_test',
    webhookSecret: () => 'whsec_test',
    stripe: () =>
      ({
        webhooks: {
          constructEventAsync: async () =>
            subscriptionEvent('customer.subscription.updated', subscription('active')),
        },
        subscriptions: { retrieve: async () => subscription('active') },
      }) as unknown as ReturnType<typeof import('../src/lib/stripe.ts').stripe>,
    claimStripeEvent: async () => true,
    upsertSubscription: async () => {
      throw new Error('database unavailable');
    },
    releaseStripeEvent: async (id) => {
      released.push(id);
    },
  });

  const response = await handler(request());
  assert.equal(response.status, 500);
  assert.deepEqual(released, ['evt_123']);
});

test('signup parses a normal multipart form through the capped reader', async () => {
  const form = new FormData();
  form.set('email', 'person@example.com');
  form.set('company_url', '');
  const body = await readSignupBody(
    new Request('https://syogun.com/api/waitlist/signup', { method: 'POST', body: form }),
  );
  assert.deepEqual(body, { email: 'person@example.com', company_url: '' });
});

test('signup rejects an oversized multipart form even without Content-Length', async () => {
  const form = new FormData();
  form.set('email', 'person@example.com');
  form.set('padding', 'x'.repeat(MAX_BODY_BYTES));
  const req = new Request('https://syogun.com/api/waitlist/signup', { method: 'POST', body: form });
  assert.equal(
    req.headers.has('content-length'),
    false,
    'test the streaming cap, not declared length',
  );
  await assert.rejects(
    readSignupBody(req),
    (error: unknown) => error instanceof HttpError && error.code === 'payload_too_large',
  );
});
