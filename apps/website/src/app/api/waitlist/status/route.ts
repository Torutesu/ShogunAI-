import { fail } from '@/lib/http';

export const runtime = 'nodejs';

/**
 * Retired with the referral program. Old status-token URLs are invalidated.
 */
export function GET() {
  return fail('not_found');
}
