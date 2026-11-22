// Thin fetch wrapper. Types come from Rust (src/api/generated, made by
// `cargo test -p rd-api`), so a server change that breaks the UI fails `tsc`.
//
// The session is an HttpOnly cookie. The browser sends it by itself, and this
// code never reads or stores it.
import type { ApiError } from './generated/ApiError'
import type { AuditEntry } from './generated/AuditEntry'
import type { DirEntry } from './generated/DirEntry'
import type { DirListing } from './generated/DirListing'
import type { EntryDetails } from './generated/EntryDetails'
import type { PathResult } from './generated/PathResult'
import type { Places } from './generated/Places'
import type { SaveResult } from './generated/SaveResult'
import type { SearchResults } from './generated/SearchResults'
import type { SortKey } from './generated/SortKey'
import type { TrashItem } from './generated/TrashItem'
import type { UploadStatus } from './generated/UploadStatus'
import type { ChangePasswordRequest } from './generated/ChangePasswordRequest'
import type { Conflict } from './generated/Conflict'
import type { LoginRequest } from './generated/LoginRequest'
import type { ServerInfo } from './generated/ServerInfo'
import type { SessionInfo } from './generated/SessionInfo'

export class ApiFailure extends Error {
  readonly code: string
  readonly status: number
  /** Seconds to wait before a retry, when the server sent `Retry-After`. */
  readonly retryAfter: number | null
  /** For a refused upload chunk: the byte the server expects next. */
  uploadOffset: number | null = null
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
  audit: () => request<AuditEntry[]>('GET', '/api/v1/audit'),
  fs: {
    list: (query: ListQuery, signal?: AbortSignal) => request<DirListing>('GET', `/api/v1/fs/list?${listParams(query)}`, undefined, signal),
    entry: (path: string) => request<DirEntry>('GET', `/api/v1/fs/entry?${new URLSearchParams({ path })}`),
    places: () => request<Places>('GET', '/api/v1/fs/places'),
    search: (q: string, signal?: AbortSignal) =>
      request<SearchResults>('GET', `/api/v1/fs/search?${new URLSearchParams({ q })}`, undefined, signal),
    mkdir: (path: string) => request<void>('POST', '/api/v1/fs/mkdir', { path }),
    rename: (from: string, to: string, conflict: Conflict = 'fail') =>
      request<PathResult>('POST', '/api/v1/fs/rename', { from, to, conflict }),
    copy: (from: string, to: string, conflict: Conflict = 'fail') =>
      request<PathResult>('POST', '/api/v1/fs/copy', { from, to, conflict }),
    remove: (path: string) => request<void>('POST', '/api/v1/fs/delete', { path }),
    properties: (path: string) => request<EntryDetails>('GET', `/api/v1/fs/properties?${new URLSearchParams({ path })}`),
    chmod: (path: string, mode: number, elevate: boolean) => request<void>('POST', '/api/v1/fs/chmod', { path, mode, elevate }),
    chown: (path: string, owner: string | null, group: string | null, elevate: boolean) =>
      request<void>('POST', '/api/v1/fs/chown', { path, owner, group, elevate }),
    compress: (from: string, to: string) => request<PathResult>('POST', '/api/v1/fs/compress', { from, to, conflict: 'keep_both' }),
    extract: (from: string, to: string) => request<PathResult>('POST', '/api/v1/fs/extract', { from, to, conflict: 'keep_both' }),
    trash: (path: string) => request<void>('POST', '/api/v1/fs/trash', { path }),
    trashList: () => request<TrashItem[]>('GET', '/api/v1/fs/trash'),
    trashRestore: (id: string, conflict: Conflict = 'fail') =>
      request<PathResult>('POST', '/api/v1/fs/trash/restore', { id, conflict }),
    trashDelete: (id: string) => request<void>('POST', '/api/v1/fs/trash/delete', { id }),
    trashEmpty: () => request<void>('POST', '/api/v1/fs/trash/empty', {}),
    uploadBegin: (dir: string, name: string, size: number, conflict: Conflict) =>
      request<UploadStatus>('POST', '/api/v1/fs/uploads', { dir, name, size, conflict }),
    uploadStatus: (id: string) => request<UploadStatus>('GET', `/api/v1/fs/uploads/${id}`),
    uploadCancel: (id: string) => request<void>('DELETE', `/api/v1/fs/uploads/${id}`),
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

/** The address that downloads a file through the root program. For a file
 * that the server itself may not read. */
export const elevatedFileUrl = (path: string) => `/api/v1/fs/download?${new URLSearchParams({ path, elevate: 'true' })}`

/** The address that sends a file or a folder as a zip archive. */
export const zipUrl = (path: string) => `/api/v1/fs/zip?${new URLSearchParams({ path })}`

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

/** Sends raw bytes: a file save or a chunk of an upload. */
async function sendBytes<T>(method: string, path: string, body: Blob | string, headers: Record<string, string>, signal?: AbortSignal): Promise<T> {
  let res: Response
  try {
    res = await fetch(path, { method, body, signal, credentials: 'same-origin', headers: { 'Content-Type': 'application/octet-stream', ...headers } })
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') {
      throw new ApiFailure(0, { code: 'aborted', message: 'The request was cancelled.' })
    }
    onNetworkFailure()
    throw new ApiFailure(0, { code: 'network', message: 'The server did not answer.' })
  }
  if (!res.ok) {
    const err = (await res.json().catch(() => null)) as ApiError | null
    const failure = new ApiFailure(res.status, err ?? { code: 'http_error', message: res.statusText })
    // For a chunk that the server already has: where to continue.
    const offset = res.headers.get('Upload-Offset')
    if (offset !== null) failure.uploadOffset = Number(offset)
    throw failure
  }
  return (await res.json()) as T
}

/** Sends the bytes of an upload from `offset`. */
export function uploadChunk(id: string, offset: number, chunk: Blob, signal?: AbortSignal): Promise<UploadStatus> {
  return sendBytes<UploadStatus>('PATCH', `/api/v1/fs/uploads/${id}?offset=${offset}`, chunk, {}, signal)
}

/** Reads a whole text file, with the version tag that a later save needs. */
export async function readTextFile(path: string, signal?: AbortSignal): Promise<{ bytes: Uint8Array; etag: string }> {
  let res: Response
  try {
    res = await fetch(fileUrl(path), { credentials: 'same-origin', signal, cache: 'no-store' })
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') {
      throw new ApiFailure(0, { code: 'aborted', message: 'The request was cancelled.' })
    }
    onNetworkFailure()
    throw new ApiFailure(0, { code: 'network', message: 'The server did not answer.' })
  }
  if (!res.ok) {
    const err = (await res.json().catch(() => null)) as ApiError | null
    throw new ApiFailure(res.status, err ?? { code: 'http_error', message: res.statusText })
  }
  return { bytes: new Uint8Array(await res.arrayBuffer()), etag: res.headers.get('ETag') ?? '' }
}

/** Saves a text file. `etag` is the version that was read. With `force`,
 * the file is written even if it changed since. Returns the new version. */
export async function saveTextFile(path: string, text: string, etag: string, force = false): Promise<string> {
  const params = new URLSearchParams({ path })
  if (force) params.set('force', 'true')
  const headers: Record<string, string> = force || !etag ? {} : { 'If-Match': etag }
  return (await sendBytes<SaveResult>('PUT', `/api/v1/fs/file?${params}`, text, headers)).etag
}
