'use client';

import { Loader2 } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { playSuccess } from '@/lib/sound';

export type WaitlistLabels = {
  placeholder: string;
  submit: string;
  errBadEmail: string;
  errRate: string;
  errNetwork: string;
  okListed: string;
};

/**
 * Waitlist form. POSTs to /api/waitlist/signup and, on success, redirects
 * to the private status URL (duplicates included).
 */
export function WaitlistForm({ refCode, labels }: { refCode?: string; labels: WaitlistLabels }) {
  const [email, setEmail] = useState('');
  const [state, setState] = useState<'idle' | 'loading' | 'error'>('idle');
  const [msg, setMsg] = useState('');

  async function onSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setState('loading');
    setMsg('');
    const honeypot = (e.currentTarget.elements.namedItem('company_url') as HTMLInputElement)?.value ?? '';
    // Plan the visitor picked in the pricing section, if any (analytics only).
    let plan: string | null = null;
    try {
      plan = window.localStorage.getItem('shogun_plan_intent');
    } catch {
      /* storage unavailable */
    }
    try {
      const res = await fetch('/api/waitlist/signup', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, ref: refCode, company_url: honeypot, plan }),
      });
      const data = await res.json();
      if (!res.ok || !data.ok) {
        setState('error');
        setMsg(data?.error === 'rate_limited' ? labels.errRate : labels.errBadEmail);
        return;
      }
      try {
        window.localStorage.removeItem('shogun_plan_intent');
      } catch {
        /* ignore */
      }
      playSuccess(); // no-op unless the visitor enabled sound
      if (data.statusUrl) {
        window.location.href = data.statusUrl;
      } else {
        setState('idle');
        setMsg(labels.okListed);
      }
    } catch {
      setState('error');
      setMsg(labels.errNetwork);
    }
  }

  return (
    <form
      onSubmit={onSubmit}
      noValidate
      className="mx-auto mt-8 flex w-full max-w-xl flex-wrap justify-center gap-3 rounded-[30px] border border-white/65 bg-white/76 p-3 shadow-[0_20px_60px_rgba(12,80,109,0.12)] backdrop-blur-md xl:mx-0 xl:justify-start"
    >
      <Input
        type="email"
        name="email"
        required
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        placeholder={labels.placeholder}
        aria-label="Email address"
        className="min-w-[220px] flex-1 border-white/80 bg-white/92 text-[16px] shadow-[inset_0_1px_0_rgba(255,255,255,0.72)] placeholder:text-[#6f8792]"
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
        className="min-w-[170px] bg-[linear-gradient(135deg,#0f5e7a,#1389af)] text-white shadow-[0_16px_34px_rgba(17,109,140,0.24)] hover:shadow-[0_20px_40px_rgba(17,109,140,0.28)]"
      >
        {state === 'loading' ? <Loader2 className="size-4 animate-spin" /> : labels.submit}
      </Button>
      {msg && (
        <p className={`basis-full px-1 text-sm ${state === 'error' ? 'text-danger' : 'text-accent-strong'}`}>{msg}</p>
      )}
    </form>
  );
}
