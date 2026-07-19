/** Shared, framework-agnostic utilities. */

/** Clamp a number to the inclusive [min, max] range. */
export function clamp(n: number, min: number, max: number): number {
  return Math.min(Math.max(n, min), max);
}

/** Strip a trailing slash from a URL/origin. */
export function trimTrailingSlash(s: string): string {
  return s.replace(/\/$/, '');
}
