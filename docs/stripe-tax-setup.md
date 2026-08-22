# Stripe Tax — setup runbook

`STRIPE_AUTOMATIC_TAX=1` turns on `automatic_tax` in Checkout
(`apps/website/src/app/api/stripe/checkout/route.ts`). **Do the dashboard work first.** With the
flag on and the account not configured, `checkout.sessions.create` throws and *every* purchase
fails — there is no degraded mode. The flag exists precisely so the code can be in the tree before
the account is ready.

Everything below is Stripe account configuration, not code. Nothing in the repo changes except
the final step.

## Why bother before we are registered anywhere

Registering to collect tax in a jurisdiction is a business decision (and a question for an
accountant, not for this document). Turning Stripe Tax on is not the same decision:

- With **no registrations**, `automatic_tax` still succeeds and computes **zero** tax everywhere.
  Nothing changes for the buyer.
- What you get is Stripe's **threshold monitoring**: it watches where your sales are accumulating
  and tells you when a jurisdiction's registration threshold is close. EU and UK B2C digital
  services have a zero threshold for non-resident sellers, so "close" can mean "the first sale".

So the sequence is: turn it on, sell, read the monitoring, then decide about registrations and
about whether a Merchant of Record (Polar / Paddle) is the cheaper answer. Turning it on later
means that decision is made on a guess instead of a number.

Note Stripe Tax carries a per-transaction fee on calculated transactions — check the current rate
on Stripe's pricing page before flipping the flag, since it changes the MoR comparison.

## Setup, in order

Each step blocks the next. Steps 1–3 are the ones that make `automatic_tax` fail if skipped.

### 1. Origin address

**Dashboard → Settings → Tax → Origin address** (Stripe moves this around; search "Tax" in
settings if the path differs).

This is the business address the sale is made *from*. Without it Stripe refuses to calculate at
all — the API error names this explicitly.

### 2. Default product tax code

**Dashboard → Settings → Tax → Default tax category** (or per-product, on each Product).

Every product being taxed needs a tax code. Set an account default so new products inherit it,
and confirm it on the two existing Products (`ShogunAI Standard`, `ShogunAI Pro` — the names in
`src/lib/pricing.ts`).

Pick from Stripe's picker rather than pasting a code from anywhere, including this document: the
SaaS category splits by business-use vs personal-use, and the split changes the rate in some
jurisdictions. Read the picker's description text for each candidate. If Standard and Pro are
meant to be sold to different audiences, they can carry different codes.

### 3. Tax behavior on every Price — the one that bites

**Dashboard → Product catalog → each Price → Tax behavior**, set to **Exclusive** (tax added on
top of the listed amount) for USD list pricing.

A Price with `tax_behavior: unspecified` makes Checkout fail once automatic tax is on. This
catches people because the Prices already exist and look fine.

`tax_behavior` can be set **once** on a Price that is currently `unspecified`; after that it is
immutable. If you set it wrong you must create a new Price and repoint the environment variable —
so check the choice before saving. All four need it:

- `STRIPE_PRICE_STANDARD_ANNUAL`
- `STRIPE_PRICE_STANDARD_MONTHLY`
- `STRIPE_PRICE_PRO_ANNUAL`
- `STRIPE_PRICE_PRO_MONTHLY`

(`billingReady()` refuses to open Checkout unless all four are configured, so a Price swapped for
a new one must have its env var updated before billing comes back.)

### 4. Registrations — only where you have decided to collect

**Dashboard → Tax → Registrations.**

Add a registration only for a jurisdiction where you are actually registered to remit. Adding one
makes Stripe start collecting tax from buyers there immediately. Leaving this empty is a valid
state and the one to start in: calculation runs, monitoring accumulates, nothing is charged.

Do not add a registration because Stripe suggests it. That is the accountant's call.

### 5. Flip the flag

Set `STRIPE_AUTOMATIC_TAX=1` on the `apps/website` deployment. In test mode first.

## Verifying

Stripe Tax settings and registrations are **separate between test and live mode** — configuring
test mode does not configure live. Do steps 1–4 in both.

In test mode, run a Checkout for each case and read the session's tax lines:

1. **A buyer in an unregistered country.** Should complete with zero tax. This is the state you
   launch in, so it is the one that must not break.
2. **A buyer in a registered country**, if you have added one. Tax appears as a separate line on
   top of the price (exclusive behaviour from step 3).
3. **A business buyer with a VAT number.** `tax_id_collection` is on, so Checkout shows the field;
   supplying a valid EU/UK VAT number should move the sale to reverse charge and drop the tax to
   zero. Stripe publishes test VAT numbers for this.
4. **A returning customer.** The code passes `customer_update: { address: 'auto' }` only when the
   session names an existing customer — Stripe rejects that parameter alongside
   `customer_creation`. Buy once, then buy again with the same email, and confirm the second
   session still opens.

If any session creation 500s, read the Stripe error message rather than guessing: it names which
of steps 1–3 is missing, in those words.

## Rolling back

Unset `STRIPE_AUTOMATIC_TAX` (or set it to anything but `1`). Checkout goes back to collecting no
tax and no billing address, immediately, with no dashboard changes needed. The `tax_behavior` you
set on the Prices in step 3 is harmless while the flag is off.

## What this does not do

Stripe Tax calculates and collects. It does **not** register you anywhere, file returns, or remit.
Those stay with you (or with a Merchant of Record, which is the alternative this data is meant to
help you evaluate — see the pricing discussion in the billing notes).
