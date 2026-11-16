// The one search for everything. Each area of the product adds a provider: a
// function from the query to a group of results. Later apps (files, services,
// containers) add their providers to `PROVIDERS`.
import { KeyRound, LogOut, Settings as SettingsIcon, SunMoon } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { APPS, RAIL_APPS } from '../apps/registry'
import { SECTIONS } from '../apps/settings/sections'
import { t } from '../i18n'
import { setPrefs } from '../theme/prefs'
import type { ShellApi } from './context'

export interface SearchItem {
  /** Unique in the whole result list. */
  key: string
  title: string
  detail: string
  icon: LucideIcon
  /** True for an app that is planned but not built. */
  later?: boolean
  run: (shell: ShellApi) => void
}

export interface SearchGroup {
  label: string
  items: SearchItem[]
}

/** `query` is trimmed and lower-case. An empty query asks for suggestions. */
export type SearchProvider = (query: string) => SearchGroup | null

const matches = (query: string, ...texts: string[]) => texts.some((s) => s.toLowerCase().includes(query))

const group = (label: string, items: SearchItem[]): SearchGroup | null => (items.length ? { label, items } : null)

const appsProvider: SearchProvider = (query) =>
  group(
    t.search.groupApps,
    RAIL_APPS.map((id) => APPS[id])
      .filter((app) => matches(query, app.name, app.description))
      .map((app) => ({
        key: `app:${app.id}`,
        title: app.name,
        detail: app.description,
        icon: app.icon,
        later: !app.built,
        run: (shell) => shell.openApp(app.id),
      })),
  )

const settingsProvider: SearchProvider = (query) => {
  if (!query) return null
  const items: SearchItem[] = []
  for (const section of SECTIONS) {
    for (const entry of section.entries) {
      if (!matches(query, entry.label, entry.keywords, section.name)) continue
      items.push({
        key: `setting:${section.id}:${entry.label}`,
        title: entry.label,
        detail: t.search.settingsPath(section.name),
        icon: SettingsIcon,
        run: (shell) => shell.openApp('settings', { section: section.id }),
      })
    }
  }
  return group(t.search.groupSettings, items.slice(0, 5))
}

const ACTIONS: (SearchItem & { keywords: string })[] = [
  {
    key: 'action:theme',
    ...t.actions.toggleTheme,
    icon: SunMoon,
    keywords: 'theme dark light mode appearance',
    run: () => {
      const dark = document.documentElement.dataset.theme === 'dark'
      setPrefs({ theme: dark ? 'light' : 'dark' })
    },
  },
  {
    key: 'action:password',
    ...t.actions.changePassword,
    icon: KeyRound,
    keywords: 'password account security',
    run: (shell) => shell.openApp('settings', { section: 'account' }),
  },
  {
    key: 'action:sign-out',
    ...t.actions.signOut,
    icon: LogOut,
    keywords: 'sign out log out logout exit',
    run: (shell) => shell.signOut(),
  },
]

const actionsProvider: SearchProvider = (query) =>
  group(
    query ? t.search.groupActions : t.search.groupSuggested,
    ACTIONS.filter((a) => matches(query, a.title, a.keywords)),
  )

const PROVIDERS: SearchProvider[] = [appsProvider, settingsProvider, actionsProvider]

export function search(rawQuery: string): SearchGroup[] {
  const query = rawQuery.trim().toLowerCase()
  return PROVIDERS.map((provider) => provider(query)).filter((g): g is SearchGroup => g !== null)
}
