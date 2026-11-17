// The sign-in page.
import { CircleAlert, Eye, EyeOff, Timer } from 'lucide-react'
import { useEffect, useState } from 'react'
import type { SubmitEvent } from 'react'
import { api, ApiFailure } from '../api/client'
import type { SessionInfo } from '../api/generated/SessionInfo'
import { errorText, t } from '../i18n'
import { LogoTile } from '../ui/Logo'

interface Props {
  /** Shown above the form, for example after the session ended. */
  notice?: string
  /** `password` is passed on, so the first-password page need not ask again. */
  onDone: (session: SessionInfo, password: string) => void
}

export function SignIn({ notice, onDone }: Props) {
  const [username, setUsername] = useState('admin')
  const [password, setPassword] = useState('')
  const [show, setShow] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Seconds left while the server refuses attempts from this address.
  const [locked, setLocked] = useState(0)

  useEffect(() => {
    if (locked <= 0) return
    const timer = setTimeout(() => setLocked((s) => s - 1), 1000)
    return () => clearTimeout(timer)
  }, [locked])

  const submit = async (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (locked > 0) return
    setBusy(true)
    setError(null)
    try {
      onDone(await api.signIn({ username, password }), password)
    } catch (err) {
      setPassword('')
      if (err instanceof ApiFailure && err.code === 'rate_limited' && err.retryAfter !== null) {
        setLocked(err.retryAfter)
      } else {
        setError(errorText(err))
      }
    } finally {
      setBusy(false)
    }
  }

  const isLocked = locked > 0
  const message = isLocked ? t.errors.rateLimited(locked) : (error ?? notice)
  const isError = isLocked || error !== null

  return (
    <div className="auth">
      <form className="auth-card" onSubmit={submit}>
        <div className="auth-server">
          <LogoTile size={40} />
          <span className="auth-server-name">
            <b>{t.product}</b>
            <span className="mono muted">{t.signIn.serverAt(window.location.host)}</span>
          </span>
        </div>
        <h1>{t.signIn.title}</h1>
        <label className="field">
          <span>{t.signIn.username}</span>
          <input
            className="input"
            name="username"
            autoComplete="username"
            autoCapitalize="none"
            spellCheck={false}
            required
            value={username}
            onChange={(e) => setUsername(e.target.value)}
          />
        </label>
        <label className="field">
          <span>{t.signIn.password}</span>
          <span className="input-wrap">
            <input
              className="input"
              name="password"
              type={show ? 'text' : 'password'}
              autoComplete="current-password"
              autoFocus
              required
              disabled={isLocked}
              aria-invalid={error !== null}
              aria-describedby={message ? 'sign-in-message' : undefined}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
            <button
              type="button"
              className="icon-btn"
              aria-label={show ? t.signIn.hidePassword : t.signIn.showPassword}
              aria-pressed={show}
              onClick={() => setShow((s) => !s)}
            >
              {show ? <EyeOff size={15} aria-hidden="true" /> : <Eye size={15} aria-hidden="true" />}
            </button>
          </span>
        </label>
        {message &&
          (isError ? (
            <div id="sign-in-message" className="alert" role="alert">
              {isLocked ? <Timer size={15} aria-hidden="true" /> : <CircleAlert size={15} aria-hidden="true" />}
              <span>{message}</span>
            </div>
          ) : (
            <p id="sign-in-message" className="muted" role="status">
              {message}
            </p>
          ))}
        <button type="submit" className="btn btn-primary btn-large" disabled={busy || isLocked}>
          {busy ? t.signIn.busy : t.signIn.submit}
        </button>
      </form>
    </div>
  )
}
