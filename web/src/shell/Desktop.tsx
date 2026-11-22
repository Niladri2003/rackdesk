// The desktop: the taskbar, the windows and everything that floats above
// them. It owns the window manager state and the keyboard shortcuts.
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from 'react'
import { api, ApiFailure } from '../api/client'
import type { SessionInfo } from '../api/generated/SessionInfo'
import { APPS, APP_IDS, RAIL_APPS } from '../apps/registry'
import type { CloseGuard } from '../apps/registry'
import { errorText, t } from '../i18n'
import { loadJson, saveJson } from '../lib/store'
import { watchConnection } from './connection'
import { ShellContext } from './context'
import type { ShellApi } from './context'
import { useQuestion } from './dialog'
import { Dialogs } from './Dialogs'
import { Launcher } from './Launcher'
import { clearNotices, notify } from './notify'
import { Rail } from './Rail'
import type { RailState, RailWindow } from './Rail'
import { Switcher } from './Switcher'
import { ConnectionBanner, Toasts } from './Toasts'
import { TransferPanel } from './TransferPanel'
import { Window } from './Window'
import type { SnapTarget } from './Window'
import { parseSaved, recentFirst, wmReducer } from './wm'
import type { AppId, Size } from './wm'

/** Below this width every window fills the desktop. */
const COMPACT_QUERY = '(max-width: 1023px)'
const RAIL_WIDTH = 64

interface Props {
  session: SessionInfo
  onSignedOut: () => void
  onSessionEnded: () => void
}

export function Desktop({ session, onSignedOut, onSessionEnded }: Props) {
  // The layout is kept per account, in this browser.
  const storageKey = `rd.windows.${session.username}`
  const [state, dispatch] = useReducer(wmReducer, undefined, () => parseSaved(loadJson(storageKey), APP_IDS))
  const [desk, setDesk] = useState<Size>(() => ({ w: window.innerWidth - RAIL_WIDTH, h: window.innerHeight }))
  const [compact, setCompact] = useState(() => window.matchMedia(COMPACT_QUERY).matches)
  const [launcher, setLauncher] = useState(false)
  const [switcher, setSwitcher] = useState<number | null>(null)
  const [snapPreview, setSnapPreview] = useState<SnapTarget>(null)
  const questionOpen = useQuestion() !== null

  const deskRef = useRef<HTMLDivElement>(null)
  const elements = useRef(new Map<number, HTMLElement>())
  const stateRef = useRef(state)
  const deskSizeRef = useRef(desk)
  const switcherRef = useRef(switcher)
  // True between "an action opened a window" and "focus moved into it".
  const pendingFocus = useRef(false)
  useEffect(() => {
    stateRef.current = state
    deskSizeRef.current = desk
    switcherRef.current = switcher
  })

  // Follow the size of the desktop area and the screen class.
  useEffect(() => {
    const el = deskRef.current
    if (!el) return
    const measure = () => setDesk({ w: el.clientWidth, h: el.clientHeight })
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(el)
    const query = window.matchMedia(COMPACT_QUERY)
    const onQuery = () => setCompact(query.matches)
    query.addEventListener('change', onQuery)
    return () => {
      observer.disconnect()
      query.removeEventListener('change', onQuery)
    }
  }, [])

  // Save the layout shortly after it changes, and when the page is left.
  useEffect(() => {
    const timer = setTimeout(() => saveJson(storageKey, { wins: state.wins }), 300)
    return () => clearTimeout(timer)
  }, [state.wins, storageKey])
  useEffect(() => {
    const save = () => saveJson(storageKey, { wins: stateRef.current.wins })
    window.addEventListener('pagehide', save)
    return () => window.removeEventListener('pagehide', save)
  }, [storageKey])

  const focusCurrent = useCallback(() => {
    const id = stateRef.current.focus
    const el = id === null ? null : elements.current.get(id)
    if (el && !el.contains(document.activeElement)) el.focus({ preventScroll: true })
  }, [])

  // Keyboard focus follows the focused window.
  useEffect(() => {
    if (!launcher && switcher === null) focusCurrent()
  }, [state.focus, launcher, switcher, focusCurrent])

  // Close checks of windows, for example an editor with unsaved changes.
  const guards = useRef(new Map<number, CloseGuard>())
  const setGuard = useCallback((id: number, guard: CloseGuard | null) => {
    if (guard) guards.current.set(id, guard)
    else guards.current.delete(id)
  }, [])
  const closeWindow = useCallback((id: number) => {
    const guard = guards.current.get(id)
    const close = () => {
      guards.current.delete(id)
      dispatch({ type: 'close', id })
    }
    if (!guard) close()
    else void guard().then((ok) => ok && close())
  }, [])

  const register = useCallback((id: number, el: HTMLElement | null) => {
    if (el) elements.current.set(id, el)
    else elements.current.delete(id)
  }, [])

  const openApp = useCallback((app: AppId, params?: Record<string, string>) => {
    pendingFocus.current = true
    const def = APPS[app]
    const existing = def.singleton ? stateRef.current.wins.find((w) => w.app === app) : undefined
    if (existing) {
      if (params) dispatch({ type: 'setParams', id: existing.id, params })
      dispatch({ type: 'focus', id: existing.id })
    } else {
      dispatch({ type: 'open', app, size: def.size, desk: deskSizeRef.current, params })
    }
  }, [])

  const signOut = useCallback(() => {
    api.signOut().then(
      () => {
        clearNotices()
        onSignedOut()
      },
      (e: unknown) => notify('error', errorText(e)),
    )
  }, [onSignedOut])

  const shell = useMemo<ShellApi>(
    () => ({ session, openApp, signOut, sessionEnded: onSessionEnded }),
    [session, openApp, signOut, onSessionEnded],
  )

  // After a menu or the search dialog closes, focus goes to the window that
  // the chosen action opened, and not back to the button.
  const onCloseAutoFocus = useCallback(
    (e: Event) => {
      if (!pendingFocus.current) return
      pendingFocus.current = false
      e.preventDefault()
      requestAnimationFrame(focusCurrent)
    },
    [focusCurrent],
  )

  const onRailApp = useCallback(
    (app: AppId) => {
      const own = recentFirst(stateRef.current.wins.filter((w) => w.app === app))
      const top = own[0]
      if (!top) openApp(app)
      else if (stateRef.current.focus === top.id && !top.min) dispatch({ type: 'minimise', id: top.id })
      else dispatch({ type: 'focus', id: top.id })
    },
    [openApp],
  )

  // When the server answers again, check that the session still exists. A
  // restart of the server ends every session.
  useEffect(
    () =>
      watchConnection(() => {
        notify('success', t.connection.restored)
        api.session().catch((e: unknown) => {
          if (e instanceof ApiFailure && e.code === 'unauthorized') onSessionEnded()
        })
      }),
    [onSessionEnded],
  )

  const commitSwitch = useCallback((index: number) => {
    const win = recentFirst(stateRef.current.wins)[index]
    setSwitcher(null)
    if (win) dispatch({ type: 'focus', id: win.id })
  }, [])

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey
      const { wins, focus } = stateRef.current
      const open = switcherRef.current

      // Alt+` opens the window switcher and moves through it.
      if (e.altKey && e.code === 'Backquote') {
        e.preventDefault()
        if (!wins.length) return
        const step = e.shiftKey ? wins.length - 1 : 1
        setSwitcher(((open ?? 0) + step) % wins.length)
        return
      }
      if (open !== null) {
        e.preventDefault()
        if (e.key === 'Escape') setSwitcher(null)
        else if (e.key === 'Enter') commitSwitch(open)
        else if (e.key === 'ArrowRight') setSwitcher((open + 1) % wins.length)
        else if (e.key === 'ArrowLeft') setSwitcher((open + wins.length - 1) % wins.length)
        return
      }
      // In a terminal, Ctrl+K is a shell key (delete to the end of the
      // line). There the search opens with Cmd+K, or with its button.
      const inTerminal = e.target instanceof Element && e.target.closest('.terminal') !== null
      if (mod && !e.altKey && e.key.toLowerCase() === 'k' && !(inTerminal && !e.metaKey)) {
        e.preventDefault()
        setLauncher((v) => !v)
        return
      }
      // Ctrl+Alt+arrows place the focused window without a pointer.
      if (e.ctrlKey && e.altKey && focus !== null && e.key.startsWith('Arrow')) {
        const win = wins.find((w) => w.id === focus)
        if (!win) return
        e.preventDefault()
        if (e.key === 'ArrowLeft') dispatch({ type: 'snap', id: focus, to: 'left' })
        else if (e.key === 'ArrowRight') dispatch({ type: 'snap', id: focus, to: 'right' })
        else if (e.key === 'ArrowUp') dispatch({ type: 'snap', id: focus, to: 'max' })
        else if (win.max || win.snap) dispatch({ type: 'snap', id: focus, to: null })
        else dispatch({ type: 'minimise', id: focus })
      }
    }
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === 'Alt' && switcherRef.current !== null) commitSwitch(switcherRef.current)
    }
    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('keyup', onKeyUp)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('keyup', onKeyUp)
    }
  }, [commitSwitch])

  // The taskbar needs 1 word per app. The words are joined into a string, so
  // the object below keeps its identity while a window only moves.
  const railKey = RAIL_APPS.map((app): RailState => {
    const own = state.wins.filter((w) => w.app === app)
    return own.some((w) => w.id === state.focus && !w.min) ? 'focused' : own.length ? 'open' : 'closed'
  }).join(',')
  const railStates = useMemo(() => {
    const words = railKey.split(',') as RailState[]
    return Object.fromEntries(RAIL_APPS.map((app, i) => [app, words[i]])) as Record<AppId, RailState>
  }, [railKey])
  const openLauncher = useCallback(() => setLauncher(true), [])

  // Windows of apps with no fixed place on the taskbar get a button each.
  // The list is made from a string for the same reason as above.
  const extraKey = JSON.stringify(
    state.wins
      .filter((w) => !RAIL_APPS.includes(w.app))
      .map((w): RailWindow => {
        const app = APPS[w.app]
        return {
          id: w.id,
          app: w.app,
          title: t.window.title(app.name, app.detail?.(w.params) ?? ''),
          state: w.id === state.focus && !w.min ? 'focused' : 'open',
        }
      }),
  )
  const extraWindows = useMemo(() => JSON.parse(extraKey) as RailWindow[], [extraKey])
  const onRailWindow = useCallback((id: number) => {
    const win = stateRef.current.wins.find((w) => w.id === id)
    if (!win) return
    if (stateRef.current.focus === id && !win.min) dispatch({ type: 'minimise', id })
    else dispatch({ type: 'focus', id })
  }, [])

  const half = Math.round(desk.w / 2)
  const preview =
    snapPreview === 'left'
      ? { left: 6, width: half - 9 }
      : snapPreview === 'right'
        ? { left: half + 3, width: desk.w - half - 9 }
        : { left: 6, width: desk.w - 12 }

  return (
    <ShellContext.Provider value={shell}>
      <div className="wall">
        {/* While the search dialog, the switcher or a question is open, the
            desktop behind it takes no input and is hidden from screen
            readers. */}
        <div className="desktop" inert={launcher || switcher !== null || questionOpen}>
          <Rail states={railStates} windows={extraWindows} onWindow={onRailWindow} onLauncher={openLauncher} onApp={onRailApp} onCloseAutoFocus={onCloseAutoFocus} />
          <main ref={deskRef} className="desk" aria-label={t.product}>
            {snapPreview && <div className="snap-preview" style={{ ...preview, top: 6, height: desk.h - 12 }} />}
            {state.wins.map((win) => (
              <Window
                key={win.id}
                win={win}
                app={APPS[win.app]}
                focused={state.focus === win.id && !win.min}
                compact={compact}
                desk={desk}
                deskRef={deskRef}
                dispatch={dispatch}
                onSnapPreview={setSnapPreview}
                register={register}
                onClose={closeWindow}
                setGuard={setGuard}
              />
            ))}
            <ConnectionBanner />
          </main>
        </div>
        {launcher && <Launcher onClose={() => setLauncher(false)} onCloseAutoFocus={onCloseAutoFocus} />}
        {switcher !== null && <Switcher wins={recentFirst(state.wins)} selected={switcher} onPick={commitSwitch} />}
        <TransferPanel />
        <Toasts />
        <Dialogs />
      </div>
    </ShellContext.Provider>
  )
}
