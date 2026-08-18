import { fail } from '@/lib/http';

export const runtime = 'nodejs';

/**
 * Retired with the referral program. Do not serve old token-protected data.
 */
export function GET() {
  return fail('not_found');
}
