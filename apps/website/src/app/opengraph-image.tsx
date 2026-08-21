import { ImageResponse } from 'next/og';
import { siteConfig } from '@/lib/site';

export const alt = `${siteConfig.name} — ${siteConfig.tagline}`;
export const size = { width: 1200, height: 630 };
export const contentType = 'image/png';

/** Dynamic Open Graph fallback. Keep it aligned with public/og-image.png. */
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
          background: 'linear-gradient(135deg, #f7f9ff 0%, #ffffff 52%, #f5f1e8 100%)',
          fontFamily: 'sans-serif',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 20 }}>
          <svg width="64" height="41" viewBox="0 0 957 614">
            {/* Inlined rather than shared with components/Logo: the OG image is rendered by
                Satori, which supports neither <use> nor <symbol>. Geometry must stay in step. */}
            <g fill="#004CFC">
              <path d="M296 254 L469 0 L469 525 Z" />
              <path d="M0 101 L276 264 L446 524 L176 390 Z" />
              <path d="M62 613 L171 413 L331 493 Z" />
              <g transform="translate(957,0) scale(-1,1)">
                <path d="M296 254 L469 0 L469 525 Z" />
                <path d="M0 101 L276 264 L446 524 L176 390 Z" />
                <path d="M62 613 L171 413 L331 493 Z" />
              </g>
            </g>
          </svg>
          <div style={{ fontSize: 40, fontWeight: 700, color: '#090b0c' }}>ShogunAI</div>
        </div>
        <div
          style={{
            marginTop: 40,
            display: 'flex',
            flexDirection: 'column',
            fontSize: 68,
            fontWeight: 700,
            lineHeight: 1.05,
            letterSpacing: '-0.02em',
            color: '#090b0c',
            maxWidth: 900,
          }}
        >
          <span>Your personal AGI</span>
          <span style={{ color: '#004cfc' }}>on your PC.</span>
        </div>
        <div style={{ marginTop: 28, fontSize: 30, color: '#50617f' }}>
          Built to finish real work.
        </div>
      </div>
    ),
    { ...size },
  );
}
