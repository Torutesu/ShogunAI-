import { siteConfig } from '@/lib/site';

/** Inline a JSON-LD block. Content is our own structured data (safe). */
export function JsonLd({ data }: { data: Record<string, unknown> }) {
  return (
    <script
      type="application/ld+json"
      // eslint-disable-next-line react/no-danger
      dangerouslySetInnerHTML={{ __html: JSON.stringify(data) }}
    />
  );
}

export const organizationSchema = {
  '@context': 'https://schema.org',
  '@type': 'Organization',
  name: siteConfig.name,
  url: siteConfig.url,
  description: siteConfig.description,
  sameAs: [
    'https://twitter.com/shogunai',
    'https://www.linkedin.com/company/shogunai',
    'https://github.com/Torutesu/ShogunAI-',
    'https://www.producthunt.com/products/shogunai',
  ],
};

export const websiteSchema = {
  '@context': 'https://schema.org',
  '@type': 'WebSite',
  name: siteConfig.name,
  url: siteConfig.url,
  inLanguage: ['en', 'ja', 'es', 'de'],
  publisher: { '@type': 'Organization', name: siteConfig.name },
};

export const softwareApplicationSchema = {
  '@context': 'https://schema.org',
  '@type': 'SoftwareApplication',
  name: siteConfig.name,
  applicationCategory: 'BusinessApplication',
  operatingSystem: 'macOS',
  description: siteConfig.description,
  featureList: [
    'Local-first work memory',
    'Natural-language recall across work context',
    'Execution across 20+ connected tools',
    'Bring your own AI keys (BYOK)',
    'Approval gates for consequential actions',
  ],
  offers: {
    '@type': 'AggregateOffer',
    lowPrice: '49',
    highPrice: '124',
    priceCurrency: 'USD',
    offerCount: '4',
  },
};

export const publisherSchema = {
  '@type': 'Organization',
  name: siteConfig.name,
  url: siteConfig.url,
  logo: {
    '@type': 'ImageObject',
    url: `${siteConfig.url}/product-icon.png`,
  },
};

export function breadcrumbSchema(items: { name: string; url: string }[]) {
  return {
    '@context': 'https://schema.org',
    '@type': 'BreadcrumbList',
    itemListElement: items.map((it, i) => ({
      '@type': 'ListItem',
      position: i + 1,
      name: it.name,
      item: it.url,
    })),
  };
}
