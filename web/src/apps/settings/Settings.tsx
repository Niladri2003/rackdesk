// The Settings app: account, appearance and about.
import { Check, CircleAlert, LogOut, Monitor, Moon, Sun } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useEffect, useState } from 'react'
import type { SubmitEvent } from 'react'
import { api, ApiFailure } from '../../api/client'
import type { ServerInfo } from '../../api/generated/ServerInfo'
import { errorText, t } from '../../i18n'
import { useShell } from '../../shell/context'
import { notify } from '../../shell/notify'
import { THEMES, setPrefs, useIsDark, usePrefs } from '../../theme/prefs'
import type { Theme } from '../../theme/prefs'
import { ACCENTS, WALLPAPERS, accentSwatch, wallpaperColor } from '../../theme/tokens'
import type { Accent } from '../../theme/tokens'
import { Lockup } from '../../ui/Logo'
import type { AppProps } from '../registry'
import { SECTIONS, sectionOf } from './sections'

const s = t.settings

export function Settings({ params, setParams }: AppProps) {
  const current = sectionOf(params.section)
  return (
    <div className="settings">
      <nav className="settings-nav" aria-label={s.sectionsLabel}>
        {SECTIONS.map((section) => {
          const Icon = section.icon
          const on = section.id === current.id
          return (
            <button
              key={section.id}
              type="button"
              className="side-item"
              aria-current={on ? 'page' : undefined}
              onClick={() => setParams({ section: section.id })}
            >
              <Icon size={15} aria-hidden="true" />
              {section.name}
            </button>
          )
        })}
      </nav>
      <div className="settings-main">
        <div className="settings-content">
          {current.id === 'account' && <Account />}
          {current.id === 'appearance' && <Appearance />}
          {current.id === 'about' && <About />}
        </div>
      </div>
    </div>
  )
}

function Heading({ title, lead }: { title: string; lead: string }) {
  return (
    <header className="settings-heading">
      <h2>{title}</h2>
      <p className="muted">{lead}</p>
    </header>
  )
}

function Account() {
  const shell = useShell()
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [confirm, setConfirm] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const name = shell.session.username

  const submit = async (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (next !== confirm) {
      setError(s.account.mismatch)
      return
    }
    setBusy(true)
    setError(null)
    try {
      await api.changePassword({ current_password: current, new_password: next })
      setCurrent('')
      setNext('')
      setConfirm('')
      notify('success', s.account.changed)
    } catch (err) {
      if (err instanceof ApiFailure && err.code === 'unauthorized') shell.sessionEnded()
      else setError(errorText(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <Heading title={s.account.name} lead={s.account.lead} />
      <div className="settings-user">
        <span className="avatar" aria-hidden="true">
          {name.slice(0, 1).toUpperCase()}
        </span>
        <span>
          <b>{name}</b>
          <span className="muted">{s.account.role}</span>
        </span>
      </div>
      <form className="settings-form" onSubmit={submit}>
        <div>
          <h3>{s.account.changeTitle}</h3>
          <p className="muted">{s.account.changeLead}</p>
        </div>
        <input type="text" name="username" autoComplete="username" value={name} readOnly hidden />
        <label className="field">
          <span>{s.account.current}</span>
          <input
            className="input"
            type="password"
            name="current-password"
            autoComplete="current-password"
            required
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
          />
        </label>
        <label className="field">
          <span>{s.account.next}</span>
          <input
            className="input"
            type="password"
            name="new-password"
            autoComplete="new-password"
            required
            minLength={12}
            maxLength={256}
            aria-describedby="settings-password-hint"
            value={next}
            onChange={(e) => setNext(e.target.value)}
          />
        </label>
        <p id="settings-password-hint" className="muted small">
          {s.account.hint}
        </p>
        <label className="field">
          <span>{s.account.confirm}</span>
          <input
            className="input"
            type="password"
            name="confirm-password"
            autoComplete="new-password"
            required
            value={confirm}
            onChange={(e) => setConfirm(e.target.value)}
          />
        </label>
        {error && (
          <div className="alert" role="alert">
            <CircleAlert size={15} aria-hidden="true" />
            <span>{error}</span>
          </div>
        )}
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? s.account.busy : s.account.submit}
        </button>
      </form>
      <div className="settings-row">
        <span>
          <b>{s.account.signOutTitle}</b>
          <span className="muted">{s.account.signOutLead}</span>
        </span>
        <button type="button" className="btn" onClick={shell.signOut}>
          <LogOut size={15} aria-hidden="true" />
          {s.account.signOutTitle}
        </button>
      </div>
    </>
  )
}

const THEME_ICON: Record<Theme, LucideIcon> = { system: Monitor, light: Sun, dark: Moon }
const THEME_LABEL: Record<Theme, string> = {
  system: s.appearance.themeSystem,
  light: s.appearance.themeLight,
  dark: s.appearance.themeDark,
}

function Appearance() {
  const prefs = usePrefs()
  const dark = useIsDark()
  const a = s.appearance
  return (
    <>
      <Heading title={a.name} lead={a.lead} />
      <div>
        <div className="settings-row">
          <span>
            <b id="settings-theme">{a.theme}</b>
            <span className="muted">{a.themeLead}</span>
          </span>
          <div className="segmented" role="radiogroup" aria-labelledby="settings-theme">
            {THEMES.map((theme) => {
              const Icon = THEME_ICON[theme]
              return (
                <button
                  key={theme}
                  type="button"
                  role="radio"
                  aria-checked={prefs.theme === theme}
                  onClick={() => setPrefs({ theme })}
                >
                  <Icon size={14} aria-hidden="true" />
                  {THEME_LABEL[theme]}
                </button>
              )
            })}
          </div>
        </div>
        <div className="settings-row">
          <span>
            <b id="settings-accent">{a.accent}</b>
            <span className="muted">{a.accents[prefs.accent]}</span>
          </span>
          <div className="swatches" role="radiogroup" aria-labelledby="settings-accent">
            {(Object.keys(ACCENTS) as Accent[]).map((accent) => (
              <button
                key={accent}
                type="button"
                role="radio"
                aria-checked={prefs.accent === accent}
                aria-label={a.accents[accent]}
                title={a.accents[accent]}
                style={{ background: accentSwatch(accent) }}
                onClick={() => setPrefs({ accent })}
              >
                {prefs.accent === accent && <Check size={13} aria-hidden="true" />}
              </button>
            ))}
          </div>
        </div>
        <div className="settings-row settings-row-stack">
          <span>
            <b id="settings-wallpaper">{a.wallpaper}</b>
            <span className="muted">{a.wallpaperLead}</span>
          </span>
          <div className="wallpapers" role="radiogroup" aria-labelledby="settings-wallpaper">
            {WALLPAPERS.map((wall) => (
              <button
                key={wall}
                type="button"
                role="radio"
                aria-checked={prefs.wallpaper === wall}
                onClick={() => setPrefs({ wallpaper: wall })}
              >
                <span style={{ background: wallpaperColor(dark, prefs.accent, wall) }} />
                {a.wallpapers[wall]}
              </button>
            ))}
          </div>
        </div>
      </div>
    </>
  )
}

function About() {
  const shell = useShell()
  const [info, setInfo] = useState<ServerInfo | null>(null)
  const [error, setError] = useState<string | null>(null)
  const { sessionEnded } = shell

  // Loaded when this section opens. Nothing polls.
  useEffect(() => {
    let live = true
    api.serverInfo().then(
      (value) => live && setInfo(value),
      (e: unknown) => {
        if (!live) return
        if (e instanceof ApiFailure && e.code === 'unauthorized') sessionEnded()
        else setError(errorText(e))
      },
    )
    return () => {
      live = false
    }
  }, [sessionEnded])

  const a = s.about
  return (
    <>
      <Lockup size={44} />
      <Heading title={a.name} lead={a.lead} />
      {error && (
        <div className="alert" role="alert">
          <CircleAlert size={15} aria-hidden="true" />
          <span>{error}</span>
        </div>
      )}
      <dl className="settings-facts" aria-busy={!info && !error}>
        <div>
          <dt>{a.hostname}</dt>
          <dd className="mono">{info ? info.hostname : <span className="skeleton" />}</dd>
        </div>
        <div>
          <dt>{a.version}</dt>
          <dd className="mono">{info ? info.version : <span className="skeleton" />}</dd>
        </div>
        <div>
          <dt>{a.licence}</dt>
          <dd className="mono">{a.licenceValue}</dd>
        </div>
      </dl>
    </>
  )
}
