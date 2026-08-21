'use client';

import { useEffect, useRef } from 'react';

const REDUCED_MOTION_PREVIEW_SECONDS = 3;
const DEMO_HOLD_SECONDS = 23.25;

export function HeroVideo({ label }: { label: string }) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const heldOnStableFrameRef = useRef(false);

  useEffect(() => {
    const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
    const syncPlayback = () => {
      const video = videoRef.current;
      if (!video) return;
      if (heldOnStableFrameRef.current) {
        video.pause();
        return;
      }
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

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    let frameCallbackId: number | undefined;

    const holdOnStableFrame = () => {
      if (heldOnStableFrameRef.current || video.currentTime < DEMO_HOLD_SECONDS) {
        return false;
      }

      heldOnStableFrameRef.current = true;
      video.pause();
      video.currentTime = DEMO_HOLD_SECONDS;
      return true;
    };

    const watchFrame = () => {
      if (holdOnStableFrame() || typeof video.requestVideoFrameCallback !== 'function') {
        return;
      }
      frameCallbackId = video.requestVideoFrameCallback(watchFrame);
    };

    const startFrameWatch = () => {
      if (
        heldOnStableFrameRef.current ||
        typeof video.requestVideoFrameCallback !== 'function' ||
        frameCallbackId !== undefined
      ) {
        return;
      }
      frameCallbackId = video.requestVideoFrameCallback(watchFrame);
    };

    const handleTimeUpdate = () => {
      holdOnStableFrame();
    };

    video.addEventListener('playing', startFrameWatch);
    video.addEventListener('timeupdate', handleTimeUpdate);
    if (!video.paused) startFrameWatch();

    return () => {
      video.removeEventListener('playing', startFrameWatch);
      video.removeEventListener('timeupdate', handleTimeUpdate);
      if (frameCallbackId !== undefined && typeof video.cancelVideoFrameCallback === 'function') {
        video.cancelVideoFrameCallback(frameCallbackId);
      }
    };
  }, []);

  return (
    <div className="hero-video-shell" data-testid="hero-product-video">
      <div className="hero-macbook">
        <div className="hero-macbook-lid">
          <div className="hero-macbook-screen">
            <video
              ref={videoRef}
              aria-label={label}
              autoPlay
              className="hero-video"
              muted
              onEnded={(event) => {
                heldOnStableFrameRef.current = true;
                event.currentTarget.pause();
              }}
              onLoadedMetadata={(event) => {
                if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
                  event.currentTarget.currentTime = REDUCED_MOTION_PREVIEW_SECONDS;
                  event.currentTarget.pause();
                }
              }}
              playsInline
              poster="/optimized/shogunheromac-v4-screen-poster.jpg"
              preload="metadata"
              tabIndex={-1}
              width={1200}
              height={782}
            >
              <source src="/optimized/shogunheromac-v4-screen.mp4" type="video/mp4" />
            </video>
          </div>
        </div>
        <div className="hero-macbook-base" aria-hidden="true" />
      </div>
    </div>
  );
}
