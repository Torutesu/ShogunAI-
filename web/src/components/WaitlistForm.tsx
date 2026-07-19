'use client';

import { useState } from 'react';

/**
 * Hero waitlist form. POSTs to /api/waitlist/signup and, on success,
 * redirects the browser to the private status URL (duplicates included).
 */
export function WaitlistForm({ refCode }: { refCode?: string }) {
  const [email, setEmail] = useState('');
  const [state, setState] = useState<'idle' | 'loading' | 'error'>('idle');
  const [msg, setMsg] = useState('');

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setState('loading');
    setMsg('');
    const form = e.currentTarget as HTMLFormElement;
    const honeypot = (form.elements.namedItem('company_url') as HTMLInputElement)?.value ?? '';

    try {
      const res = await fetch('/api/waitlist/signup', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, ref: refCode, company_url: honeypot }),
      });
      const data = await res.json();
      if (!res.ok || !data.ok) {
        setState('error');
        setMsg(
          data?.error === 'rate_limited'
            ? 'Too many attempts. Try again in a minute.'
            : 'That email looks off. Check it and try again.',
        );
        return;
      }
      if (data.statusUrl) {
        window.location.href = data.statusUrl;
      } else {
        setState('idle');
        setMsg('You’re on the list.');
      }
    } catch {
      setState('error');
      setMsg('Network hiccup. Try again.');
    }
  }

  return (
    <form className="wl-form" onSubmit={onSubmit} noValidate>
      <input
        className="wl-form__input"
        type="email"
        name="email"
        placeholder="you@work.com"
        aria-label="Email address"
        required
        value={email}
        onChange={(e) => setEmail(e.target.value)}
      />
      {/* Honeypot — hidden from real users */}
      <input className="wl-form__hp" type="text" name="company_url" tabIndex={-1} autoComplete="off" aria-hidden="true" />
      <button className="btn btn-primary" type="submit" disabled={state === 'loading'}>
        {state === 'loading' ? <span className="spinner" /> : 'Get early access'}
      </button>
      {msg && (
        <div className={`wl-msg ${state === 'error' ? 'wl-msg--err' : 'wl-msg--ok'}`} style={{ flexBasis: '100%' }}>
          {msg}
        </div>
      )}
    </form>
  );
}
