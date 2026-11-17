// What the shell offers to apps and to search providers.
import { createContext, useContext } from 'react'
import type { SessionInfo } from '../api/generated/SessionInfo'
import type { AppId } from './wm'

export interface ShellApi {
  session: SessionInfo
  /** Opens an app, or focuses its window if it allows only one. */
  openApp: (app: AppId, params?: Record<string, string>) => void
  signOut: () => void
  /** Call when a request fails with `unauthorized`. */
  sessionEnded: () => void
}

export const ShellContext = createContext<ShellApi | null>(null)

export function useShell(): ShellApi {
  const shell = useContext(ShellContext)
  if (!shell) throw new Error('useShell is used outside the desktop')
  return shell
}
