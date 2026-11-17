// The taskbar: a vertical rail on the left edge with the search button, the
// apps, notifications, the clock and the account menu.
import { Bell, LogOut, Settings as SettingsIcon } from 'lucide-react'
import { DropdownMenu, Popover } from 'radix-ui'
import { memo, useEffect, useState } from 'react'
import { APPS, RAIL_APPS } from '../apps/registry'
import { t } from '../i18n'
import { LogoMark } from '../ui/Logo'
import { countRender } from '../lib/renderCount'
import { useShell } from './context'
import { MOD_KEY, NOTICE_ICON } from './icons'
import { clearNotices, markAllRead, useNotices } from './notify'
import type { AppId } from './wm'

/** `focused`: a window of the app has focus. `open`: it has a window. */
export type RailState = 'focused' | 'open' | 'closed'

/** A window whose app has no fixed place on the taskbar, such as a preview. */
export interface RailWindow {
  id: number
  app: AppId
  title: string
  state: RailState
}

interface Props {
  states: Record<AppId, RailState>
  windows: RailWindow[]
  /** Click on such a window: focus it, or minimise it if it is in front. */
  onWindow: (id: number) => void
  onLauncher: () => void
  /** Click on an app: open it, focus it, or minimise it if it is in front. */
  onApp: (app: AppId) => void
  /** Called when the account menu has closed and focus can move. */
  onCloseAutoFocus: (e: Event) => void
}

/** Memoised: it does not draw again while a window moves. */
export const Rail = memo(function Rail({ states, windows, onWindow, onLauncher, onApp, onCloseAutoFocus }: Props) {
  countRender('rail')
  const shell = useShell()
  const { list, unread } = useNotices()
  const now = useClock()

  return (
    <nav className="rail" aria-label={t.rail.label}>
      <button
        type="button"
        className="rail-launcher"
        aria-label={t.rail.launcher}
        title={t.rail.launcherHint(MOD_KEY)}
        onClick={onLauncher}
      >
        <LogoMark size={24} />
      </button>

      {RAIL_APPS.map((id) => {
        const app = APPS[id]
        const state = states[id]
        const Icon = app.icon
        return (
          <button
            key={id}
            type="button"
            className="rail-app"
            data-state={state}
            aria-label={app.name}
            aria-pressed={state === 'focused'}
            title={app.name}
            onClick={() => onApp(id)}
          >
            <Icon size={19} aria-hidden="true" />
            <span className="rail-indicator" aria-hidden="true" />
          </button>
        )
      })}

      {windows.length > 0 && <span className="rail-divider" aria-hidden="true" />}
      {windows.map((w) => {
        const Icon = APPS[w.app].icon
        return (
          <button
            key={w.id}
            type="button"
            className="rail-app"
            data-state={w.state}
            aria-label={w.title}
            aria-pressed={w.state === 'focused'}
            title={w.title}
            onClick={() => onWindow(w.id)}
          >
            <Icon size={18} aria-hidden="true" />
            <span className="rail-indicator" aria-hidden="true" />
          </button>
        )
      })}

      <span className="rail-spacer" />

      <Popover.Root onOpenChange={(open) => open && markAllRead()}>
        <Popover.Trigger asChild>
          <button
            type="button"
            className="rail-app rail-bell"
            aria-label={unread ? t.rail.unread(unread) : t.rail.notifications}
            title={t.rail.notifications}
          >
            <Bell size={18} aria-hidden="true" />
            {unread > 0 && (
              <span className="rail-badge" aria-hidden="true">
                {unread > 9 ? '9+' : unread}
              </span>
            )}
          </button>
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content className="panel notices" side="right" align="end" sideOffset={10} aria-label={t.notifications.title}>
            <div className="notices-head">
              <b>{t.notifications.title}</b>
              {list.length > 0 && (
                <button type="button" className="text-btn" onClick={clearNotices}>
                  {t.notifications.clear}
                </button>
              )}
            </div>
            <ul className="notices-list">
              {list.map((n) => {
                const Icon = NOTICE_ICON[n.kind]
                return (
                  <li key={n.id}>
                    <Icon size={16} data-kind={n.kind} aria-hidden="true" />
                    <span>
                      <span>{n.text}</span>
                      <span className="notice-time">{timeAgo(n.time, now)}</span>
                    </span>
                  </li>
                )
              })}
              {list.length === 0 && <li className="notices-empty">{t.notifications.empty}</li>}
            </ul>
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>

      <time className="rail-clock" aria-label={t.rail.time} dateTime={now.toISOString()}>
        {pad(now.getHours())}
        <br />
        {pad(now.getMinutes())}
      </time>

      <DropdownMenu.Root modal={false}>
        <DropdownMenu.Trigger asChild>
          <button type="button" className="rail-avatar" aria-label={t.rail.account} title={shell.session.username}>
            {shell.session.username.slice(0, 1).toUpperCase()}
          </button>
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal>
          <DropdownMenu.Content className="panel menu" side="right" align="end" sideOffset={10} onCloseAutoFocus={onCloseAutoFocus}>
            <DropdownMenu.Label className="menu-head">
              <b>{shell.session.username}</b>
              <span className="mono muted">{window.location.host}</span>
            </DropdownMenu.Label>
            <DropdownMenu.Item className="menu-item" onSelect={() => shell.openApp('settings')}>
              <SettingsIcon size={15} aria-hidden="true" />
              {t.userMenu.settings}
            </DropdownMenu.Item>
            <DropdownMenu.Item className="menu-item" onSelect={() => shell.signOut()}>
              <LogOut size={15} aria-hidden="true" />
              {t.userMenu.signOut}
            </DropdownMenu.Item>
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
    </nav>
  )
})

const pad = (n: number) => String(n).padStart(2, '0')

function timeAgo(time: number, now: Date): string {
  const minutes = Math.floor((now.getTime() - time) / 60000)
  if (minutes < 1) return t.notifications.justNow
  if (minutes < 60) return t.notifications.minutesAgo(minutes)
  const d = new Date(time)
  return t.notifications.at(`${pad(d.getHours())}:${pad(d.getMinutes())}`)
}

/** The current time. It updates once, at the start of each minute. */
function useClock(): Date {
  const [now, setNow] = useState(() => new Date())
  useEffect(() => {
    const timer = setTimeout(() => setNow(new Date()), 60000 - (now.getTime() % 60000) + 50)
    return () => clearTimeout(timer)
  }, [now])
  return now
}
