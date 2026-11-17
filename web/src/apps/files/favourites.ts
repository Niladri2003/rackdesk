// Favourite folders. They are kept in this browser, 1 list per account.
import { useCallback, useState } from 'react'
import { loadJson, saveJson } from '../../lib/store'

const MAX = 30

function read(key: string): string[] {
  const raw = loadJson(key)
  if (!Array.isArray(raw)) return []
  // Stored data is untrusted: keep only strings that look like UI paths.
  return raw.filter((p): p is string => typeof p === 'string' && p.startsWith('/') && p.length < 4096).slice(0, MAX)
}

export function useFavourites(username: string): { list: string[]; toggle: (path: string) => void } {
  const key = `rd.files.favourites.${username}`
  const [list, setList] = useState(() => read(key))
  const toggle = useCallback(
    (path: string) => {
      // Read again first: another Files window may have changed the list.
      const now = read(key)
      const next = now.includes(path) ? now.filter((p) => p !== path) : [...now, path].slice(-MAX)
      saveJson(key, next)
      setList(next)
    },
    [key],
  )
  return { list, toggle }
}
