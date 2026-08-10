'use client';

import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';

/**
 * The one screen where a licence key is ever shown (issue #8). Copy-to-clipboard is not a nicety
 * here: a mistyped key is the difference between a paid Mac and a locked one, and the key uses a
 * no-lookalikes alphabet precisely because people retype it anyway.
 */
export function LicenseKeyCard({
  licenseKey,
  planLine,
}: {
  licenseKey: string;
  planLine: string | null;
}) {
  const [copied, setCopied] = useState(false);

  const copy = () => {
    void navigator.clipboard
      .writeText(licenseKey)
      .then(() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 2000);
      })
      .catch(() => undefined);
  };

  return (
    <Card className="grid gap-5 p-8">
      <div className="grid gap-2 text-center">
        <h1 className="font-display text-2xl font-semibold">You&rsquo;re subscribed</h1>
        {planLine && <p className="text-muted text-sm">{planLine}</p>}
      </div>

      <div className="grid gap-2">
        <div className="text-muted text-xs font-semibold tracking-[0.08em] uppercase">
          Licence key
        </div>
        <div className="border-border bg-cloud flex items-center justify-between gap-3 rounded-lg border px-4 py-3">
          <code className="text-ink font-mono text-[15px] tracking-wide select-all">{licenseKey}</code>
          <Button size="sm" variant="secondary" onClick={copy}>
            {copied ? 'Copied' : 'Copy'}
          </Button>
        </div>
        <p className="text-muted text-sm">
          Open ShogunAI → Settings → Plan &amp; billing, and paste this key to activate this Mac.
          It&rsquo;s also in your receipt email.
        </p>
      </div>

      <div className="text-muted grid gap-1 text-sm">
        <p>Manage your card, invoices, plan or cancellation from the same settings panel.</p>
      </div>
    </Card>
  );
}
