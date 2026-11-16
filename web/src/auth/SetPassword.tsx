// The page shown while the generated initial password is still in use. The
// server refuses every other API call until the password is changed.
import { Circle, CircleAlert, CircleCheck, KeyRound } from 'lucide-react'
import { useState } from 'react'
import type { SubmitEvent } from 'react'
import { api, ApiFailure } from '../api/client'
import type { SessionInfo } from '../api/generated/SessionInfo'
import { errorText, t } from '../i18n'

const MIN_LENGTH = 12

interface Props {
  session: SessionInfo
  /** The password typed at sign-in. `null` after a page reload: the page then
   * asks for it. It is held in memory only. */
  currentPassword: string | null
  onDone: (session: SessionInfo) => void
  onSessionEnded: () => void
}

export function SetPassword({ session, currentPassword, onDone, onSessionEnded }: Props) {
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [confirm, setConfirm] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const longEnough = next.length >= MIN_LENGTH
  const same = longEnough && next === confirm
  const askCurrent = currentPassword === null

  const submit = async (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (!same) return
    setBusy(true)
    setError(null)
    try {
      onDone(await api.changePassword({ current_password: currentPassword ?? current, new_password: next }))
    } catch (err) {
      if (err instanceof ApiFailure && err.code === 'unauthorized') onSessionEnded()
      else setError(errorText(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="auth">
      <form className="auth-card" onSubmit={submit}>
        <div className="auth-eyebrow mono muted">
          <KeyRound size={14} aria-hidden="true" />
          {t.setPassword.eyebrow(session.username)}
        </div>
        <div className="auth-lead">
          <h1>{t.setPassword.title}</h1>
          <p>{t.setPassword.body}</p>
        </div>
        {/* Password managers use this hidden field to store the new password
            under the right account. */}
        <input type="text" name="username" autoComplete="username" value={session.username} readOnly hidden />
        {askCurrent && (
          <label className="field">
            <span>{t.setPassword.current}</span>
            <input
              className="input"
              type="password"
              name="current-password"
              autoComplete="current-password"
              autoFocus
              required
              value={current}
              onChange={(e) => setCurrent(e.target.value)}
            />
          </label>
        )}
        <label className="field">
          <span>{t.setPassword.next}</span>
          <input
            className="input"
            type="password"
            name="new-password"
            autoComplete="new-password"
            autoFocus={!askCurrent}
            required
            maxLength={256}
            aria-describedby="set-password-rules"
            value={next}
            onChange={(e) => setNext(e.target.value)}
          />
        </label>
        <label className="field">
          <span>{t.setPassword.confirm}</span>
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
        <ul id="set-password-rules" className="auth-rules">
          <Rule ok={longEnough} text={t.setPassword.ruleLength} />
          <Rule ok={same} text={t.setPassword.ruleMatch} />
        </ul>
        {error && (
          <div className="alert" role="alert">
            <CircleAlert size={15} aria-hidden="true" />
            <span>{error}</span>
          </div>
        )}
        <button type="submit" className="btn btn-primary btn-large" disabled={busy || !same}>
          {busy ? t.setPassword.busy : t.setPassword.submit}
        </button>
      </form>
    </div>
  )
}

function Rule({ ok, text }: { ok: boolean; text: string }) {
  return (
    <li data-ok={ok}>
      {ok ? <CircleCheck size={15} aria-hidden="true" /> : <Circle size={15} aria-hidden="true" />}
      {text}
    </li>
  )
}
