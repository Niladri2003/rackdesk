// Text shown for each error code from the API. The UI matches on the code,
// never on the message the server sent.
import { ApiFailure } from './api/client'

const TEXT: Record<string, string> = {
  invalid_credentials: 'Wrong username or password.',
  wrong_password: 'The current password is wrong.',
  password_too_short: 'The new password must have at least 12 characters.',
  password_too_long: 'The new password must have at most 256 characters.',
  password_too_common: 'This password is on the list of common passwords. Choose another one.',
  password_unchanged: 'The new password must differ from the current one.',
  csrf_rejected: 'The browser sent this request in a way the server does not accept. Reload the page.',
  unauthorized: 'Your session has ended. Sign in again.',
  network: 'The server did not answer. Check that rackdeskd is running.',
}

export function errorText(e: unknown): string {
  if (e instanceof ApiFailure) {
    if (e.code === 'rate_limited') {
      const wait = e.retryAfter === null ? 'later' : `in ${e.retryAfter} s`
      return `Too many failed attempts. Try again ${wait}.`
    }
    return TEXT[e.code] ?? `The request failed (${e.code}).`
  }
  return 'The request failed.'
}
