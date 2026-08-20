import { NextRequest, NextResponse } from 'next/server';
import { isLocale, LOCALE_COOKIE } from '@/i18n/config';

/** Keep locale-prefixed SEO URLs, server rendering, and the UI cookie in sync. */
export function middleware(request: NextRequest) {
  const host = request.headers.get('host')?.split(':')[0].toLowerCase();
  if (host === 'www.shogunaios.com' || (host === 'shogunaios.com' && request.nextUrl.protocol === 'http:')) {
    const canonical = new URL(request.nextUrl);
    canonical.protocol = 'https:';
    canonical.hostname = 'shogunaios.com';
    return NextResponse.redirect(canonical, 301);
  }

  const firstSegment = request.nextUrl.pathname.split('/')[1];
  const response = isLocale(firstSegment)
    ? (() => {
        const requestHeaders = new Headers(request.headers);
        requestHeaders.set('x-shogun-locale', firstSegment);
        const next = NextResponse.next({ request: { headers: requestHeaders } });
        next.cookies.set(LOCALE_COOKIE, firstSegment, { path: '/', maxAge: 31_536_000, sameSite: 'lax' });
        return next;
      })()
    : NextResponse.next();

  if (host === 'shogunaios.com') {
    response.headers.set('Strict-Transport-Security', 'max-age=31536000; includeSubDomains; preload');
  }
  response.headers.set('X-Content-Type-Options', 'nosniff');
  response.headers.set('Referrer-Policy', 'strict-origin-when-cross-origin');
  response.headers.set('Permissions-Policy', 'camera=(), microphone=(), geolocation=()');
  return response;
}

export const config = {
  matcher: ['/((?!_next/static|_next/image|favicon.ico).*)'],
};
