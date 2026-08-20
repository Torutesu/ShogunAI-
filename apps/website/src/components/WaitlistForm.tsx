'use client';

import { Loader2 } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { playSuccess } from '@/lib/sound';
import { cn } from '@/lib/utils';

export type WaitlistLabels = {
  placeholder: string;
  submit: string;
  noCost: string;
  errBadEmail: string;
  errRate: string;
  errSave: string;
  errNetwork: string;
  okListed: string;
};

type Tone = 'default' | 'cta';

/**
 * Minimal waitlist form. The only user action is a privacy-safe email signup.
 */
export function WaitlistForm({
  labels,
  tone = 'default',
  className,
}: {
  labels: WaitlistLabels;
  tone?: Tone;
  className?: string;
}) {
  const [email, setEmail] = useState('');
  const [state, setState] = useState<'idle' | 'loading' | 'error'>('idle');
  const [msg, setMsg] = useState('');
  const isCta = tone === 'cta';

  async function onSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setMsg('');
    const emailInput = e.currentTarget.elements.namedItem('email') as HTMLInputElement;
    if (!emailInput.checkValidity()) {
      setState('error');
      setMsg(labels.errBadEmail);
      return;
    }

    setState('loading');
    const honeypot = (e.currentTarget.elements.namedItem('company_url') as HTMLInputElement)?.value ?? '';
    try {
      const res = await fetch('/api/waitlist/signup', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, company_url: honeypot }),
        cache: 'no-store',
      });
      // A proxy can return a non-JSON error page. Treat that as a save error,
      // not as a network failure, so the user receives an actionable message.
      const data = await res.json().catch(() => null) as { ok?: boolean; error?: string } | null;
      if (!res.ok || !data?.ok) {
        setState('error');
        setMsg(data?.error === 'rate_limited' ? labels.errRate : data?.error === 'bad_request' ? labels.errBadEmail : labels.errSave);
        return;
      }
      playSuccess(); // no-op unless the visitor enabled sound
      setState('idle');
      setEmail('');
      setMsg(labels.okListed);
    } catch {
      setState('error');
      setMsg(labels.errNetwork);
    }
  }

  return (
    <div className={cn('w-full', className)}>
      <form
        onSubmit={onSubmit}
        action="/api/waitlist/signup"
        method="post"
        noValidate
        className={cn(
          'waitlist-form mx-auto mt-8 flex w-full max-w-xl flex-wrap justify-center gap-3 p-3 backdrop-blur-md xl:mx-0 xl:justify-start',
          isCta
            ? 'rounded-[30px] border border-white/18 bg-[linear-gradient(180deg,rgba(255,255,255,0.12),rgba(255,255,255,0.08))] shadow-[0_24px_60px_rgba(3,8,20,0.24)]'
            : 'waitlist-form-default rounded-[28px] border border-[#d5e7ee] bg-[linear-gradient(180deg,rgba(255,255,255,0.94),rgba(249,253,255,0.9))] shadow-[0_20px_60px_rgba(12,80,109,0.10)] sm:flex-nowrap',
        )}
      >
      <Input
        type="email"
        name="email"
        required
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        placeholder={labels.placeholder}
        aria-label="Email address"
        className={cn(`w-full min-w-0 flex-[1_1_100%] text-[16px] sm:flex-1 sm:min-w-[220px] ${
          isCta
            ? '!border-[#d7dee7]/35 !bg-[#f7fafc] !text-[#102534] shadow-[inset_0_1px_0_rgba(255,255,255,0.88)] placeholder:!text-[#6f8792]'
            : 'border-[#d2e1e8] bg-white shadow-[inset_0_1px_0_rgba(255,255,255,0.8)] placeholder:text-[#6f8792]'
        }`, isCta && 'basis-full min-w-0 flex-none')}
      />
      <input
        type="text"
        name="company_url"
        tabIndex={-1}
        autoComplete="off"
        aria-hidden="true"
        className="absolute left-[-9999px] size-px overflow-hidden"
      />
      <Button
        type="submit"
        disabled={state === 'loading'}
        className={cn(`min-w-[170px] ${
          isCta
            ? 'border border-[#f1d8a8]/60 bg-[linear-gradient(135deg,var(--cta-start),var(--cta-end))] text-[var(--cta-ink)] shadow-[0_18px_34px_rgba(115,78,20,0.22)] hover:shadow-[0_22px_40px_rgba(115,78,20,0.28)]'
            : 'border border-[#f1d8a8]/60 bg-[linear-gradient(135deg,var(--cta-start),var(--cta-end))] text-[var(--cta-ink)] shadow-[0_16px_34px_rgba(115,78,20,0.18)] hover:shadow-[0_20px_40px_rgba(115,78,20,0.24)]'
        }`, isCta && 'ml-auto')}
      >
        {state === 'loading' ? <Loader2 className="size-4 animate-spin" /> : labels.submit}
      </Button>
      {msg && (
        <p className={`basis-full px-1 text-sm ${state === 'error' ? 'text-danger' : isCta ? 'text-white/88' : 'text-accent-strong'}`}>{msg}</p>
      )}
      </form>
      {/* In the hero this line sits over the artwork, so it carries its own
        * scrim rather than relying on whatever pixels happen to be behind it. */}
      <p className={`mt-2 text-center text-[11px] leading-relaxed ${isCta ? 'text-white/58' : 'text-muted'}`}>
        <span className={isCta ? '' : 'inline-block rounded-full bg-white/70 px-2.5 py-1 backdrop-blur-sm'}>
          {labels.noCost}
        </span>
      </p>
    </div>
  );
}
