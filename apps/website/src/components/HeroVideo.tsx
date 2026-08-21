'use client';

import { useEffect, useRef } from 'react';

const VIDEO_LOOP_START_SECONDS = 4;
const REDUCED_MOTION_PREVIEW_SECONDS = 4;

export function HeroVideo({ label }: { label: string }) {
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
    const syncPlayback = () => {
      const video = videoRef.current;
      if (!video) return;
      if (motion.matches) {
        if (video.readyState >= HTMLMediaElement.HAVE_METADATA) {
          video.currentTime = REDUCED_MOTION_PREVIEW_SECONDS;
        }
        video.pause();
        return;
      }
      void video.play().catch(() => undefined);
    };

    syncPlayback();
    motion.addEventListener('change', syncPlayback);
    return () => motion.removeEventListener('change', syncPlayback);
  }, []);

  return (
    <div className="hero-video-shell" data-testid="hero-product-video">
      <video
        ref={videoRef}
        aria-label={label}
        autoPlay
        className="hero-video"
        muted
        onEnded={(event) => {
          if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
            event.currentTarget.pause();
            return;
          }
          event.currentTarget.currentTime = VIDEO_LOOP_START_SECONDS;
          void event.currentTarget.play().catch(() => undefined);
        }}
        onLoadedMetadata={(event) => {
          if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
            event.currentTarget.currentTime = REDUCED_MOTION_PREVIEW_SECONDS;
            event.currentTarget.pause();
          }
        }}
        playsInline
        poster="/optimized/shogunheromac1200-closed.png"
        preload="metadata"
        tabIndex={-1}
        width={1200}
        height={904}
      >
        <source
          src="/optimized/shogunheromac1200-alpha.mov"
          type='video/quicktime; codecs="hvc1"'
        />
        <source src="/optimized/shogunheromac1200-alpha.webm" type="video/webm" />
      </video>
    </div>
  );
}
