const ALLOWED_DOMAINS = new Set([
  'linear.app',
  'vercel.com',
  'perplexity.ai',
  'figma.com',
  'slack.com',
  'google.com',
  'notion.so',
  'github.com',
  'openai.com',
  'anthropic.com',
  'discord.com',
  'dropbox.com',
  'asana.com',
  'gmail.com',
  'calendar.google.com',
  'teams.microsoft.com',
  'outlook.com',
  'zoom.us',
  'telegram.org',
  'whatsapp.com',
  'messenger.com',
  'x.com',
  'linkedin.com',
  'intercom.com',
  'drive.google.com',
  'onedrive.live.com',
  'confluence.com',
  'box.com',
  'coda.io',
  'airtable.com',
  'evernote.com',
  'obsidian.md',
  'miro.com',
  'clickup.com',
  'jira.com',
  'trello.com',
  'monday.com',
  'framer.com',
  'webflow.com',
  'canva.com',
  'sketch.com',
  'adobe.com',
  'gitlab.com',
  'cursor.com',
  'gemini.google.com',
  'copilot.microsoft.com',
  'x.ai',
  'mistral.ai',
  'cohere.com',
  'deepseek.com',
  'together.ai',
  'groq.com',
  'openrouter.ai',
  'huggingface.co',
  'replicate.com',
  'airbnb.com',
  'stripe.com',
  'ramp.com',
  'salesforce.com',
  'ibm.com',
  'grok.com',
  'spacex.com',
  'paper.co',
  'instagram.com',
  'ycombinator.com',
  'diabrowser.com',
  'nike.com',
  'harvard.edu',
  'mit.edu',
  'ucla.edu',
  'ubc.ca',
  'u-tokyo.ac.jp',
  'tsukuba.ac.jp',
  'sequoiacap.com',
  'a16z.com',
  'foundersfund.com',
  'accel.com',
  'rippling.com',
  'coinbase.com',
  'cmux.com',
  'granola.ai',
  'glean.com',
  'gong.io',
  'setlog.com',
  'mckinsey.com',
  'apple.com',
  'cloudflare.com',
  'theseed.vc',
]);

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

// Use the official Simple Icons artwork as the unauthenticated fallback. The
// Google favicon endpoint returns tiny square app icons, which look incorrect
// in the logo marquee. Domains without a matching Simple Icon still fall back
// to the published site favicon below.
const SIMPLE_ICON_SLUGS: Record<string, string> = {
  'linear.app': 'linear',
  'vercel.com': 'vercel',
  'perplexity.ai': 'perplexity',
  'figma.com': 'figma',
  'slack.com': 'slack',
  'google.com': 'google',
  'notion.so': 'notion',
  'github.com': 'github',
  'openai.com': 'openai',
  'anthropic.com': 'anthropic',
  'discord.com': 'discord',
  'dropbox.com': 'dropbox',
  'asana.com': 'asana',
  'airbnb.com': 'airbnb',
  'stripe.com': 'stripe',
  'ramp.com': 'ramp',
  'salesforce.com': 'salesforce',
  'ibm.com': 'ibm',
  'spacex.com': 'spacex',
  'instagram.com': 'instagram',
  'ycombinator.com': 'ycombinator',
  'nike.com': 'nike',
  'mit.edu': 'mit',
  'ucla.edu': 'ucla',
  'apple.com': 'apple',
  'cloudflare.com': 'cloudflare',
};

/**
 * Keeps the Logo.dev token on the Worker while browsers receive only a
 * same-origin image URL. The allowlist prevents this route becoming a proxy.
 */
export async function GET(_request: Request, context: { params: Promise<{ domain: string }> }) {
  const { domain } = await context.params;
  if (!ALLOWED_DOMAINS.has(domain)) return new Response(null, { status: 404 });

  const token = process.env.LOGO_DEV_TOKEN?.trim();

  try {
    // Prefer the transparent, recognizable brand mark. Logo.dev remains a
    // useful fallback for brands that are not covered by Simple Icons.
    let upstream: Response | null = null;
    const slug = SIMPLE_ICON_SLUGS[domain];
    if (slug) {
      const simpleIcon = await fetch(`https://cdn.simpleicons.org/${slug}`, {
        headers: { Accept: 'image/svg+xml,image/*;q=0.8' },
      });
      if (simpleIcon.ok && simpleIcon.body) upstream = simpleIcon;
    }

    if (!upstream && token) {
      const logoDev = await fetch(
        `https://img.logo.dev/${domain}?token=${encodeURIComponent(token)}&format=png&theme=light&retina=true&fallback=404`,
        { headers: { Accept: 'image/png,image/*;q=0.8' } },
      );
      if (logoDev.ok && logoDev.body) upstream = logoDev;
    }

    // Google serves the site's published favicon as the last fallback. The
    // domain is still constrained by the allowlist, so this remains a closed
    // brand-asset endpoint rather than an open proxy.
    upstream ??= await fetch(
      `https://www.google.com/s2/favicons?domain_url=${encodeURIComponent(`https://${domain}`)}&sz=128`,
      { headers: { Accept: 'image/png,image/*;q=0.8' } },
    );

    if (!upstream.ok || !upstream.body) return new Response(null, { status: 404 });

    return new Response(upstream.body, {
      headers: {
        'Content-Type': upstream.headers.get('content-type') ?? 'image/png',
        // Brand marks change infrequently; cache at the edge without serving
        // stale markup or exposing the provider token.
        'Cache-Control': 'public, max-age=86400, s-maxage=604800, stale-while-revalidate=86400',
        'X-Content-Type-Options': 'nosniff',
      },
    });
  } catch (error) {
    console.error('Logo.dev logo proxy error:', error);
    return new Response(null, { status: 404 });
  }
}
