// Thin fetch wrapper. Types come from Rust (src/api/generated, made by
// `cargo test -p wd-api`), so a server change that breaks the UI fails `tsc`.
import type { ApiError } from './generated/ApiError'
import type { ServerInfo } from './generated/ServerInfo'

export class ApiFailure extends Error {
  readonly code: string
  readonly status: number
  constructor(status: number, err: ApiError) {
    super(err.message)
    this.code = err.code
    this.status = status
  }
}

async function get<T>(path: string): Promise<T> {
  const res = await fetch(path, { credentials: 'same-origin' })
  if (!res.ok) {
    const body = (await res.json().catch(() => null)) as ApiError | null
    throw new ApiFailure(res.status, body ?? { code: 'http_error', message: res.statusText })
  }
  return (await res.json()) as T
}

export const api = {
  serverInfo: () => get<ServerInfo>('/api/v1/server'),
}
