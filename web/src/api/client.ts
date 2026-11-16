// Thin fetch wrapper. Types come from Rust (src/api/generated, made by
// `cargo test -p rd-api`), so a server change that breaks the UI fails `tsc`.
//
// The session is an HttpOnly cookie. The browser sends it by itself, and this
// code never reads or stores it.
import type { ApiError } from './generated/ApiError'
import type { ChangePasswordRequest } from './generated/ChangePasswordRequest'
import type { LoginRequest } from './generated/LoginRequest'
import type { ServerInfo } from './generated/ServerInfo'
import type { SessionInfo } from './generated/SessionInfo'

export class ApiFailure extends Error {
  readonly code: string
  readonly status: number
  /** Seconds to wait before a retry, when the server sent `Retry-After`. */
  readonly retryAfter: number | null
  constructor(status: number, err: ApiError, retryAfter: number | null = null) {
    super(err.message)
    this.code = err.code
    this.status = status
    this.retryAfter = retryAfter
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  let res: Response
  try {
    res = await fetch(path, {
      method,
      credentials: 'same-origin',
      headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    })
  } catch {
    throw new ApiFailure(0, { code: 'network', message: 'The server did not answer.' })
  }
  if (!res.ok) {
    const err = (await res.json().catch(() => null)) as ApiError | null
    const retryAfter = Number(res.headers.get('Retry-After'))
    throw new ApiFailure(
      res.status,
      err ?? { code: 'http_error', message: res.statusText },
      Number.isFinite(retryAfter) && retryAfter > 0 ? retryAfter : null,
    )
  }
  if (res.status === 204) return undefined as T
  return (await res.json()) as T
}

export const api = {
  session: () => request<SessionInfo>('GET', '/api/v1/session'),
  signIn: (body: LoginRequest) => request<SessionInfo>('POST', '/api/v1/session', body),
  signOut: () => request<void>('DELETE', '/api/v1/session'),
  changePassword: (body: ChangePasswordRequest) =>
    request<SessionInfo>('POST', '/api/v1/session/password', body),
  serverInfo: () => request<ServerInfo>('GET', '/api/v1/server'),
}
