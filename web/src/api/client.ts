// Thin fetch wrapper. Types come from Rust (src/api/generated, made by
// `cargo test -p rd-api`), so a server change that breaks the UI fails `tsc`.
//
// The session is an HttpOnly cookie. The browser sends it by itself, and this
// code never reads or stores it.
import type { ApiError } from './generated/ApiError'
import type { DirEntry } from './generated/DirEntry'
import type { DirListing } from './generated/DirListing'
import type { Places } from './generated/Places'
import type { SearchResults } from './generated/SearchResults'
import type { SortKey } from './generated/SortKey'
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

let onNetworkFailure: () => void = () => {}

/** The shell registers a function here to learn that a request got no answer. */
export function setNetworkFailureHandler(handler: () => void): void {
  onNetworkFailure = handler
}

/** True when the server answers. It needs no session. */
export async function serverIsUp(): Promise<boolean> {
  try {
    const res = await fetch('/healthz', { cache: 'no-store' })
    return res.ok
  } catch {
    return false
  }
}

async function request<T>(method: string, path: string, body?: unknown, signal?: AbortSignal): Promise<T> {
  let res: Response
  try {
    res = await fetch(path, {
      method,
      signal,
      credentials: 'same-origin',
      headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    })
  } catch (e) {
    // A request that the caller cancelled is not a lost connection.
    if (e instanceof DOMException && e.name === 'AbortError') {
      throw new ApiFailure(0, { code: 'aborted', message: 'The request was cancelled.' })
    }
    onNetworkFailure()
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
  fs: {
    list: (query: ListQuery, signal?: AbortSignal) => request<DirListing>('GET', `/api/v1/fs/list?${listParams(query)}`, undefined, signal),
    entry: (path: string) => request<DirEntry>('GET', `/api/v1/fs/entry?${new URLSearchParams({ path })}`),
    places: () => request<Places>('GET', '/api/v1/fs/places'),
    search: (q: string, signal?: AbortSignal) =>
      request<SearchResults>('GET', `/api/v1/fs/search?${new URLSearchParams({ q })}`, undefined, signal),
  },
}

export interface ListQuery {
  path: string
  offset: number
  limit: number
  sort: SortKey
  desc: boolean
  hidden: boolean
  /** Keeps entries whose name contains this text. */
  filter: string
}

function listParams(q: ListQuery): string {
  const params = new URLSearchParams({
    path: q.path,
    offset: String(q.offset),
    limit: String(q.limit),
    sort: q.sort,
    desc: String(q.desc),
    hidden: String(q.hidden),
  })
  if (q.filter) params.set('q', q.filter)
  return params.toString()
}

/** The address of a file's content. With `inline`, the server lets the
 * browser show images, video, audio and PDF in the page. Every other type is
 * always sent as a download. */
export function fileUrl(path: string, inline = false): string {
  const params = new URLSearchParams({ path })
  if (inline) params.set('inline', 'true')
  return `/api/v1/fs/download?${params}`
}

/** Reads the start of a file as bytes, for a text preview. An empty file
 * gives an empty array. `total` is the full size of the file in bytes. */
export async function readFileStart(path: string, maxBytes: number, signal?: AbortSignal): Promise<{ bytes: Uint8Array; total: number }> {
  let res: Response
  try {
    res = await fetch(fileUrl(path), { credentials: 'same-origin', signal, headers: { Range: `bytes=0-${maxBytes - 1}` } })
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') {
      throw new ApiFailure(0, { code: 'aborted', message: 'The request was cancelled.' })
    }
    onNetworkFailure()
    throw new ApiFailure(0, { code: 'network', message: 'The server did not answer.' })
  }
  // 416: the file has 0 bytes, so no range of it exists.
  if (res.status === 416) return { bytes: new Uint8Array(), total: 0 }
  if (!res.ok) {
    const err = (await res.json().catch(() => null)) as ApiError | null
    throw new ApiFailure(res.status, err ?? { code: 'http_error', message: res.statusText })
  }
  const bytes = new Uint8Array(await res.arrayBuffer())
  const total = Number(res.headers.get('Content-Range')?.split('/')[1] ?? bytes.length)
  return { bytes, total }
}
