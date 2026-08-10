import { ImageResponse } from 'next/og';
import { siteConfig } from '@/lib/site';

export const alt = `${siteConfig.name} — ${siteConfig.tagline}`;
export const size = { width: 1200, height: 630 };
export const contentType = 'image/png';

/** Dynamic Open Graph image. Self-contained (no external assets). */
export default function OpengraphImage() {
  return new ImageResponse(
    (
      <div
        style={{
          width: '100%',
          height: '100%',
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'center',
          padding: '80px',
          background: 'linear-gradient(135deg, #d8f6ff 0%, #f7fdff 45%, #ffffff 100%)',
          fontFamily: 'sans-serif',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 20 }}>
          <svg width="64" height="64" viewBox="0 0 1000 644">
            {/* Inlined rather than shared with components/Logo: the OG image is rendered by
                Satori, which supports neither <use> nor <symbol>. Geometry must stay in step. */}
            <g fill="#0B4DFF">
              <path d="M497 4 L307 266 L487 552 Z" />
              <path d="M0 109 L312 279 L422 531 L179 415 Z" />
              <path d="M179 435 L370 508 L62 644 Z" />
              <g transform="translate(1000,0) scale(-1,1)">
                <path d="M497 4 L307 266 L487 552 Z" />
                <path d="M0 109 L312 279 L422 531 L179 415 Z" />
                <path d="M179 435 L370 508 L62 644 Z" />
              </g>
            </g>
          </svg>
          <div style={{ fontSize: 40, fontWeight: 700, color: '#090b0c' }}>ShogunAI</div>
        </div>
        <div
          style={{
            marginTop: 40,
            fontSize: 68,
            fontWeight: 700,
            lineHeight: 1.05,
            letterSpacing: '-0.02em',
            color: '#090b0c',
            maxWidth: 900,
          }}
        >
          The AI that remembers your day — and acts on it.
        </div>
        <div style={{ marginTop: 28, fontSize: 30, color: '#5f6b73' }}>
          Memory that captures your day. Execution that acts on it.
        </div>
      </div>
    ),
    { ...size },
  );
}
