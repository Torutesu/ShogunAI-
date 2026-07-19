import { drizzle } from 'drizzle-orm/postgres-js';
import postgres from 'postgres';
import * as schema from './schema';

const connectionString =
  process.env.DATABASE_URL ??
  'postgres://postgres:postgres@localhost:5432/shogun_waitlist';

// Reuse a single client across hot reloads / serverless invocations.
const globalForDb = globalThis as unknown as { _pg?: ReturnType<typeof postgres> };
const client = globalForDb._pg ?? postgres(connectionString, { max: 10 });
if (process.env.NODE_ENV !== 'production') globalForDb._pg = client;

export const db = drizzle(client, { schema });
export { client };
export * from './schema';
