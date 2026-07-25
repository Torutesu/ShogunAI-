/**
 * Logo.dev image URLs — https://www.logo.dev/docs/logo-images/introduction
 *
 * No SDK: every logo is a plain cacheable CDN image. We look brands up by
 * domain, which is the most stable identifier (name/ticker lookups can resolve
 * to the wrong company for generic names like "Linear" or "Loom").
 *
 * The token is a *publishable* key — it rides along in the image URL and is
 * visible in page source by design. Override per environment with
 * NEXT_PUBLIC_LOGO_DEV_TOKEN; never put a secret (sk_) key here.
 */
const TOKEN = process.env.NEXT_PUBLIC_LOGO_DEV_TOKEN || 'pk_FdU2PFccSXmDgjZxLsxNxw';

type LogoOptions = {
  /** Rendered edge in px, 1–800. Defaults to the API default of 128. */
  size?: number;
  /** `jpg` is the API default; we prefer `png` so logos keep their alpha. */
  format?: 'png' | 'webp' | 'jpg';
  /** Server-side desaturation — cheaper and more consistent than a CSS filter. */
  greyscale?: boolean;
  /** Render at 2x for high-DPI screens. */
  retina?: boolean;
  /** `monogram` (default) never breaks; `404` lets you swap in your own art. */
  fallback?: 'monogram' | '404';
};

export function logoUrl(domain: string, opts: LogoOptions = {}): string {
  const { size, format = 'png', greyscale, retina, fallback } = opts;

  const q = new URLSearchParams({ token: TOKEN, format });
  if (size) q.set('size', String(size));
  if (greyscale) q.set('greyscale', 'true');
  if (retina) q.set('retina', 'true');
  if (fallback) q.set('fallback', fallback);

  return `https://img.logo.dev/${domain}?${q.toString()}`;
}
