/**
 * Shared domain types for ShogunAI. Extend as code migrates out of apps/website.
 */

export type Locale = 'en' | 'ja' | 'es' | 'de';

/** Waitlist participant status (mirrors apps/website/src/db/schema.ts). */
export type ParticipantStatus = 'pending' | 'invited' | 'converted';
