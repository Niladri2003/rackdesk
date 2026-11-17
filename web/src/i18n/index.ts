// The active language. Components import `t` from here and never hold text
// of their own.
import { ApiFailure } from '../api/client'
import { en } from './en'
import type { Messages } from './en'

export const t: Messages = en

/** Text for a failed request. It matches on the error code, never on the
 * message the server sent. */
export function errorText(e: unknown): string {
  if (!(e instanceof ApiFailure)) return t.errors.generic
  if (e.code === 'rate_limited') return t.errors.rateLimited(e.retryAfter)
  const known = t.errors as unknown as Record<string, unknown>
  const text = known[e.code]
  return typeof text === 'string' ? text : t.errors.unknown(e.code)
}
