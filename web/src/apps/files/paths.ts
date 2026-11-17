// Path and number helpers for the file browser. Paths here are UI paths:
// they start with "/" and are relative to the files root on the server.

export const joinPath = (dir: string, name: string) => (dir === '/' ? `/${name}` : `${dir}/${name}`)

export function parentPath(path: string): string {
  const i = path.lastIndexOf('/')
  return i <= 0 ? '/' : path.slice(0, i)
}

export const baseName = (path: string) => (path === '/' ? '/' : path.slice(path.lastIndexOf('/') + 1))

/** Removes double and trailing slashes, and makes the path start with "/". */
export function normalisePath(raw: string): string {
  const parts = raw.split('/').filter((p) => p !== '' && p !== '.')
  return `/${parts.join('/')}`
}

/** "/var/log" gives [{name: "var", path: "/var"}, {name: "log", path: "/var/log"}]. */
export function crumbs(path: string): { name: string; path: string }[] {
  const out: { name: string; path: string }[] = []
  let at = ''
  for (const name of path.split('/').filter(Boolean)) {
    at += `/${name}`
    out.push({ name, path: at })
  }
  return out
}

const UNITS = ['KB', 'MB', 'GB', 'TB', 'PB']

/** 1536 gives "1.5 KB". Uses multiples of 1024. */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  let value = bytes
  let unit = -1
  do {
    value /= 1024
    unit++
  } while (value >= 1024 && unit < UNITS.length - 1)
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${UNITS[unit]}`
}

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' })

/** `seconds` is Unix time, or null when the filesystem gives none. */
export const formatTime = (seconds: number | null) => (seconds === null ? '' : dateFormat.format(seconds * 1000))
