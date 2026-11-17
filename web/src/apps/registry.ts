// The list of apps. Each app has a manifest here, its window content, and
// (in shell/search.ts) its search results.
//
// A window holds 1 of 3 kinds of content. Only `builtin` is implemented. The
// other 2 exist so that adding them later does not change the window code.
import { Eye, FilePen, Folder, Settings as SettingsIcon, SquareTerminal } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { ComponentType } from 'react'
import { t } from '../i18n'
import type { AppId, Size } from '../shell/wm'
import { Editor } from './editor/Editor'
import { TRASH } from './files/changes'
import { Files } from './files/Files'
import { baseName } from './files/paths'
import { Placeholder } from './Placeholder'
import { Preview } from './preview/Preview'
import { Settings } from './settings/Settings'
import { sectionName } from './settings/sections'

/** What an app receives. It does not get the position or size of its
 * window, so moving or resizing the window does not redraw the app. */
export interface AppProps {
  app: AppDef
  /** Data kept for this window, for example the Settings section to show. */
  params: Record<string, string>
  setParams: (params: Record<string, string>) => void
  /** Sets a check that runs when the user closes the window. It returns
   * false to keep the window open, for example for unsaved changes. `null`
   * removes the check. */
  setCloseGuard: (guard: CloseGuard | null) => void
}

export type CloseGuard = () => Promise<boolean>

export type WindowContent =
  /** A React component from this bundle. */
  | { kind: 'builtin'; component: ComponentType<AppProps> }
  /** A web UI of a local program, shown in a frame. Not implemented. */
  | { kind: 'web'; url: string }
  /** A remote GUI app, streamed as video. Not implemented. */
  | { kind: 'stream' }

export interface AppDef {
  id: AppId
  name: string
  description: string
  icon: LucideIcon
  /** Size of a new window, before it is fitted to the desktop. */
  size: Size
  /** True if only 1 window of this app can be open. */
  singleton: boolean
  /** False while the app is a placeholder. Search marks it "Later". */
  built: boolean
  content: WindowContent
  /** Text after the app name in the title bar. */
  detail?: (params: Record<string, string>) => string
}

export const APPS: Record<AppId, AppDef> = {
  files: {
    id: 'files',
    ...t.apps.files,
    icon: Folder,
    size: { w: 1080, h: 660 },
    singleton: false,
    built: true,
    content: { kind: 'builtin', component: Files },
    detail: (params) => (params.path === TRASH ? t.files.trash : params.path ? baseName(params.path) : ''),
  },
  editor: {
    id: 'editor',
    ...t.apps.editor,
    icon: FilePen,
    size: { w: 820, h: 580 },
    singleton: false,
    built: true,
    content: { kind: 'builtin', component: Editor },
    detail: (params) => (params.path ? baseName(params.path) : ''),
  },
  preview: {
    id: 'preview',
    ...t.apps.preview,
    icon: Eye,
    size: { w: 860, h: 600 },
    singleton: false,
    built: true,
    content: { kind: 'builtin', component: Preview },
    detail: (params) => (params.path ? baseName(params.path) : ''),
  },
  terminal: {
    id: 'terminal',
    ...t.apps.terminal,
    icon: SquareTerminal,
    size: { w: 900, h: 560 },
    singleton: false,
    built: false,
    content: { kind: 'builtin', component: Placeholder },
  },
  settings: {
    id: 'settings',
    ...t.apps.settings,
    icon: SettingsIcon,
    size: { w: 920, h: 640 },
    singleton: true,
    built: true,
    content: { kind: 'builtin', component: Settings },
    detail: (params) => sectionName(params.section),
  },
}

/** Apps on the taskbar, in order. */
export const RAIL_APPS: AppId[] = ['files', 'terminal', 'settings']
export const APP_IDS = Object.keys(APPS)
