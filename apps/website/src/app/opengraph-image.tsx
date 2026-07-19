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
          <svg width="64" height="64" viewBox="0 0 100 100">
            <path
              d="M66 20 L34 34 L66 60 L34 80"
              fill="none"
              stroke="#0aa5f4"
              strokeWidth="26"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
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
