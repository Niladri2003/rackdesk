// Loads a folder listing in pages. The list can have 100,000 rows, so only
// the pages near the visible rows are requested.
import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '../../api/client'
import type { DirEntry } from '../../api/generated/DirEntry'
import type { SortKey } from '../../api/generated/SortKey'

export const PAGE_SIZE = 200

export interface ListingQuery {
  path: string
  sort: SortKey
  desc: boolean
  hidden: boolean
  filter: string
}

interface Data {
  key: string
  /** Number of entries in the folder. `null` until the first page arrives. */
  total: number | null
  pages: Map<number, DirEntry[]>
  error: unknown
}

export interface Listing {
  loading: boolean
  total: number
  error: unknown
  /** The entry at a row, or `undefined` while its page is loading. */
  entry: (index: number) => DirEntry | undefined
  /** Requests the pages that hold rows `first` to `last`. */
  need: (first: number, last: number) => void
  reload: () => void
}

export function useListing(query: ListingQuery): Listing {
  const key = JSON.stringify(query)
  const [version, setVersion] = useState(0)
  const [data, setData] = useState<Data>({ key: '', total: null, pages: new Map(), error: null })
  const requested = useRef(new Set<number>())
  const current = useRef({ key, query })
  useEffect(() => {
    current.current = { key, query }
  })

  const load = useCallback((page: number, signal?: AbortSignal) => {
    const { key: forKey, query: q } = current.current
    if (requested.current.has(page)) return
    requested.current.add(page)
    api.fs.list({ ...q, offset: page * PAGE_SIZE, limit: PAGE_SIZE }, signal).then(
      (listing) => {
        if (current.current.key !== forKey) return
        setData((d) => {
          const pages = d.key === forKey ? new Map(d.pages) : new Map<number, DirEntry[]>()
          pages.set(page, listing.entries)
          return { key: forKey, total: listing.total, pages, error: null }
        })
      },
      (error: unknown) => {
        if (current.current.key !== forKey || signal?.aborted) return
        requested.current.delete(page)
        setData({ key: forKey, total: null, pages: new Map(), error })
      },
    )
  }, [])

  // A new folder, sort order or filter starts from the first page.
  useEffect(() => {
    current.current = { key, query: JSON.parse(key) as ListingQuery }
    requested.current = new Set()
    const abort = new AbortController()
    load(0, abort.signal)
    return () => abort.abort()
  }, [key, version, load])

  const fresh = data.key === key
  const entry = useCallback(
    (index: number) => (fresh ? data.pages.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE] : undefined),
    [fresh, data.pages],
  )
  const need = useCallback(
    (first: number, last: number) => {
      for (let page = Math.floor(first / PAGE_SIZE); page <= Math.floor(last / PAGE_SIZE); page++) load(page)
    },
    [load],
  )
  const reload = useCallback(() => setVersion((v) => v + 1), [])

  return {
    loading: !fresh || (data.total === null && data.error === null),
    total: fresh ? (data.total ?? 0) : 0,
    error: fresh ? data.error : null,
    entry,
    need,
    reload,
  }
}
