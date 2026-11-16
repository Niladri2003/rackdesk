// Appearance settings: theme, accent and wallpaper. They are kept in this
// browser (localStorage), not on the server.
import { useEffect, useState } from 'react'
import { createStore, loadJson, saveJson, useStore } from '../lib/store'
import { ACCENTS, WALLPAPERS, makeTokens } from './tokens'
import type { Accent, Wallpaper } from './tokens'

export const THEMES = ['system', 'light', 'dark'] as const
export type Theme = (typeof THEMES)[number]

export interface Prefs {
  theme: Theme
  accent: Accent
  wallpaper: Wallpaper
}

const KEY = 'rd.prefs'
const DEFAULTS: Prefs = { theme: 'system', accent: 'periwinkle', wallpaper: 'tinted' }

/** Stored data is untrusted: keep only known values. */
function parse(raw: unknown): Prefs {
  const r = (raw ?? {}) as Partial<Record<keyof Prefs, unknown>>
  return {
    theme: THEMES.includes(r.theme as Theme) ? (r.theme as Theme) : DEFAULTS.theme,
    accent: typeof r.accent === 'string' && r.accent in ACCENTS ? (r.accent as Accent) : DEFAULTS.accent,
    wallpaper: WALLPAPERS.includes(r.wallpaper as Wallpaper) ? (r.wallpaper as Wallpaper) : DEFAULTS.wallpaper,
  }
}

const store = createStore<Prefs>(parse(loadJson(KEY)))

export const usePrefs = () => useStore(store)
export const getPrefs = () => store.get()

export function setPrefs(patch: Partial<Prefs>): void {
  store.set((prev) => ({ ...prev, ...patch }))
  saveJson(KEY, store.get())
}

const darkQuery = () => window.matchMedia('(prefers-color-scheme: dark)')

/** True when the resolved theme is dark. Follows the device when the theme
 * is "system". */
export function useIsDark(): boolean {
  const { theme } = usePrefs()
  const [deviceDark, setDeviceDark] = useState(() => darkQuery().matches)
  useEffect(() => {
    const q = darkQuery()
    const onChange = () => setDeviceDark(q.matches)
    q.addEventListener('change', onChange)
    return () => q.removeEventListener('change', onChange)
  }, [])
  return theme === 'dark' || (theme === 'system' && deviceDark)
}

/** Writes the tokens to the root element. Menus and dialogs render outside
 * the app's own element, so the variables must be on the document root. */
export function useApplyTokens(): void {
  const prefs = usePrefs()
  const dark = useIsDark()
  useEffect(() => {
    const root = document.documentElement
    const tokens = makeTokens(dark, prefs.accent, prefs.wallpaper)
    for (const [name, value] of Object.entries(tokens)) root.style.setProperty(name, value)
    root.style.colorScheme = dark ? 'dark' : 'light'
    root.dataset.theme = dark ? 'dark' : 'light'
  }, [dark, prefs.accent, prefs.wallpaper])
}
