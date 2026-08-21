'use client';

import { useEffect, useRef } from 'react';

const VIDEO_START_SECONDS = 4;

export function HeroVideo({ label }: { label: string }) {
  const videoRef = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
    const syncPlayback = () => {
      const video = videoRef.current;
      if (!video) return;
      if (motion.matches) {
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
          event.currentTarget.currentTime = VIDEO_START_SECONDS;
          void event.currentTarget.play().catch(() => undefined);
        }}
        onLoadedMetadata={(event) => {
          event.currentTarget.currentTime = VIDEO_START_SECONDS;
        }}
        playsInline
        poster="/optimized/shogunheromac1200-poster.jpg"
        preload="metadata"
        tabIndex={-1}
        width={1200}
        height={904}
      >
        <source src="/optimized/shogunheromac1200.mp4" type="video/mp4" />
      </video>
    </div>
  );
}
