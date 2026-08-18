import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  poweredByHeader: false,
  outputFileTracingRoot: path.join(__dirname, '../..'),
  images: {
    formats: ['image/avif', 'image/webp'],
  },
  experimental: {
    optimizePackageImports: ['lucide-react', 'motion'],
  },
  // PostHog is proxied same-origin so the CSP's `connect-src 'self'` holds and
  // ad blockers do not drop the analytics path (carried over from main).
  async rewrites() {
    return [
      { source: '/ingest/static/:path*', destination: 'https://us-assets.i.posthog.com/static/:path*' },
      { source: '/ingest/array/:path*', destination: 'https://us-assets.i.posthog.com/array/:path*' },
      { source: '/ingest/:path*', destination: 'https://us.i.posthog.com/:path*' },
    ];
  },
  async redirects() {
    // Comparison content was pulled and the blog collapsed to two tags
    // (Ideas, Product). These URLs were indexed, so send them somewhere real
    // instead of letting them 404.
    const locale = '(en|ja|es|de)';
    const pulledPosts = 'shogunai-vs-notion|shogunai-vs-mem|shogunai-vs-glean|best-ai-memory-tools-for-knowledge-workers';
    return [
      { source: '/compare', destination: '/features', permanent: true },
      { source: `/:locale${locale}/compare`, destination: '/:locale/features', permanent: true },
      { source: '/compare/:slug*', destination: '/features', permanent: true },
      { source: `/:locale${locale}/compare/:slug*`, destination: '/:locale/features', permanent: true },
      { source: `/blog/:slug(${pulledPosts})`, destination: '/blog', permanent: true },
      { source: `/:locale${locale}/blog/:slug(${pulledPosts})`, destination: '/:locale/blog', permanent: true },
      { source: '/blog/category/:slug(ai-memory|privacy)', destination: '/blog/category/ideas', permanent: true },
      { source: `/:locale${locale}/blog/category/:slug(ai-memory|privacy)`, destination: '/:locale/blog/category/ideas', permanent: true },
      { source: '/blog/category/work-context', destination: '/blog/category/product', permanent: true },
      { source: `/:locale${locale}/blog/category/work-context`, destination: '/:locale/blog/category/product', permanent: true },
      { source: '/blog/category/comparisons', destination: '/blog', permanent: true },
      { source: `/:locale${locale}/blog/category/comparisons`, destination: '/:locale/blog', permanent: true },
    ];
  },
  async headers() {
    return [
      {
        source: '/:path*',
        headers: [
          { key: 'X-Content-Type-Options', value: 'nosniff' },
          { key: 'X-Frame-Options', value: 'DENY' },
          { key: 'Referrer-Policy', value: 'strict-origin-when-cross-origin' },
          { key: 'Permissions-Policy', value: 'camera=(), microphone=(), geolocation=(), payment=()' },
          {
            key: 'Content-Security-Policy',
            // Next.js needs inline bootstrap scripts/styles. Every other
            // resource type is restricted to this origin or the two explicit
            // logo providers used by the LP.
            value: "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https://img.logo.dev https://cdn.brandfetch.io; font-src 'self' data:; connect-src 'self'; frame-src 'none'; base-uri 'self'; object-src 'none'; frame-ancestors 'none'; form-action 'self'; upgrade-insecure-requests",
          },
        ],
      },
    ];
  },
};

export default nextConfig;
