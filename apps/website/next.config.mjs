/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  poweredByHeader: false,
  images: {
    formats: ['image/avif', 'image/webp'],
    // Logo.dev brand marks. The trust bar uses a plain <img> (the CDN already
    // sizes and caches them, and Workers has no image optimizer), but this
    // keeps next/image usable for logos elsewhere.
    remotePatterns: [{ protocol: 'https', hostname: 'img.logo.dev' }],
  },
  experimental: {
    optimizePackageImports: ['lucide-react', 'motion'],
  },
};

export default nextConfig;
