// The sections of the Settings app, and what the one search can find in each.
import { History, Info, Palette, User } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { t } from '../../i18n'

type SectionId = 'account' | 'appearance' | 'activity' | 'about'

interface Section {
  id: SectionId
  name: string
  icon: LucideIcon
  /** What the one search can find in this section. */
  entries: { label: string; keywords: string }[]
}

const s = t.settings

export const SECTIONS: Section[] = [
  {
    id: 'account',
    name: s.account.name,
    icon: User,
    entries: [
      { label: s.account.changeTitle, keywords: 'password security' },
      { label: s.account.signOutTitle, keywords: 'sign out log out session' },
    ],
  },
  {
    id: 'appearance',
    name: s.appearance.name,
    icon: Palette,
    entries: [
      { label: s.appearance.theme, keywords: 'dark light mode' },
      { label: s.appearance.accent, keywords: 'color colour' },
      { label: s.appearance.wallpaper, keywords: 'background' },
    ],
  },
  {
    id: 'activity',
    name: s.activity.name,
    icon: History,
    entries: [{ label: s.activity.name, keywords: 'audit log history changes sign-in' }],
  },
  {
    id: 'about',
    name: s.about.name,
    icon: Info,
    entries: [{ label: s.about.version, keywords: 'version about licence license' }],
  },
]

export const sectionOf = (id: string | undefined): Section => SECTIONS.find((x) => x.id === id) ?? SECTIONS[0]

export const sectionName = (id: string | undefined) => sectionOf(id).name
