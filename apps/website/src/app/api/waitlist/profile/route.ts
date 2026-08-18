import { fail } from '@/lib/http';

export const runtime = 'nodejs';

/**
 * Retired with the referral program. Do not accept old bearer-token writes.
 */
export function POST() {
  return fail('not_found');
}
