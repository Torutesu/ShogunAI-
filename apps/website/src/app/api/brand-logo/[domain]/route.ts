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

/**
 * Keeps the Logo.dev token on the Worker while browsers receive only a
 * same-origin image URL. The allowlist prevents this route becoming a proxy.
 */
export async function GET(_request: Request, context: { params: Promise<{ domain: string }> }) {
  const { domain } = await context.params;
  if (!ALLOWED_DOMAINS.has(domain)) return new Response(null, { status: 404 });

  const token = process.env.LOGO_DEV_TOKEN?.trim();

  try {
    const logoDev = token
      ? await fetch(
          `https://img.logo.dev/${domain}?token=${encodeURIComponent(token)}&format=png&theme=light&retina=true&fallback=404`,
          { headers: { Accept: 'image/png,image/*;q=0.8' } },
        )
      : null;

    // Google serves the site's published favicon when Logo.dev is not
    // configured. The domain is still constrained by the allowlist above, so
    // this remains a closed brand-asset endpoint rather than an open proxy.
    const upstream =
      logoDev?.ok && logoDev.body
        ? logoDev
        : await fetch(
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
