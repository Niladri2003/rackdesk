// Plain pages for sign-in and the forced password change. The desktop shell
// replaces the signed-in page later (docs/design/).
import { useCallback, useEffect, useState } from 'react'
import type { SubmitEvent } from 'react'
import { api, ApiFailure } from './api/client'
import type { ServerInfo } from './api/generated/ServerInfo'
import type { SessionInfo } from './api/generated/SessionInfo'
import { errorText } from './messages'

type View =
  | { name: 'loading' }
  | { name: 'unreachable'; message: string }
  | { name: 'signed-out'; notice?: string }
  | { name: 'signed-in'; session: SessionInfo; changing: boolean }

export default function App() {
  const [view, setView] = useState<View>({ name: 'loading' })

  useEffect(() => {
    api.session().then(
      (session) => setView({ name: 'signed-in', session, changing: false }),
      (e: unknown) => {
        if (e instanceof ApiFailure && e.code === 'unauthorized') {
          setView({ name: 'signed-out' })
        } else {
          setView({ name: 'unreachable', message: errorText(e) })
        }
      },
    )
  }, [])

  const signedIn = (session: SessionInfo) => setView({ name: 'signed-in', session, changing: false })
  const signedOut = useCallback(() => setView({ name: 'signed-out' }), [])
  const sessionEnded = useCallback(
    () => setView({ name: 'signed-out', notice: 'Your session has ended. Sign in again.' }),
    [],
  )

  return (
    <main className="page">
      <h1>RackDesk</h1>
      {view.name === 'loading' && <p>Connecting...</p>}
      {view.name === 'unreachable' && (
        <p className="error" role="alert">
          {view.message}
        </p>
      )}
      {view.name === 'signed-out' && <SignIn notice={view.notice} onDone={signedIn} />}
      {view.name === 'signed-in' &&
        (view.session.must_change_password || view.changing ? (
          <ChangePassword
            forced={view.session.must_change_password}
            onDone={signedIn}
            onCancel={() => signedIn(view.session)}
            onSessionEnded={sessionEnded}
          />
        ) : (
          <Home
            session={view.session}
            onChangePassword={() => setView({ ...view, changing: true })}
            onSignedOut={signedOut}
            onSessionEnded={sessionEnded}
          />
        ))}
    </main>
  )
}

function SignIn({ notice, onDone }: { notice?: string; onDone: (s: SessionInfo) => void }) {
  const [username, setUsername] = useState('admin')
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const submit = async (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    setBusy(true)
    setError(null)
    try {
      onDone(await api.signIn({ username, password }))
    } catch (err) {
      setError(errorText(err))
      setPassword('')
    } finally {
      setBusy(false)
    }
  }

  return (
    <form className="form" onSubmit={submit}>
      <h2>Sign in</h2>
      {notice && <p className="muted">{notice}</p>}
      <label>
        Username
        <input
          name="username"
          autoComplete="username"
          autoCapitalize="none"
          spellCheck={false}
          required
          value={username}
          onChange={(e) => setUsername(e.target.value)}
        />
      </label>
      <label>
        Password
        <input
          name="password"
          type="password"
          autoComplete="current-password"
          autoFocus
          required
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
      </label>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <button type="submit" disabled={busy}>
        {busy ? 'Signing in...' : 'Sign in'}
      </button>
    </form>
  )
}

function ChangePassword({
  forced,
  onDone,
  onCancel,
  onSessionEnded,
}: {
  forced: boolean
  onDone: (s: SessionInfo) => void
  onCancel: () => void
  onSessionEnded: () => void
}) {
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [repeat, setRepeat] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const submit = async (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    if (next !== repeat) {
      setError('The 2 new passwords are not the same.')
      return
    }
    setBusy(true)
    setError(null)
    try {
      onDone(await api.changePassword({ current_password: current, new_password: next }))
    } catch (err) {
      if (err instanceof ApiFailure && err.code === 'unauthorized') {
        onSessionEnded()
        return
      }
      setError(errorText(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <form className="form" onSubmit={submit}>
      <h2>Change password</h2>
      {forced && (
        <p className="muted">
          You signed in with the password that the server generated. Choose your own password to continue.
        </p>
      )}
      <label>
        Current password
        <input
          name="current-password"
          type="password"
          autoComplete="current-password"
          autoFocus
          required
          value={current}
          onChange={(e) => setCurrent(e.target.value)}
        />
      </label>
      <label>
        New password
        <input
          name="new-password"
          type="password"
          autoComplete="new-password"
          required
          minLength={12}
          maxLength={256}
          aria-describedby="password-rule"
          value={next}
          onChange={(e) => setNext(e.target.value)}
        />
      </label>
      <p id="password-rule" className="muted hint">
        At least 12 characters.
      </p>
      <label>
        New password again
        <input
          name="new-password-repeat"
          type="password"
          autoComplete="new-password"
          required
          value={repeat}
          onChange={(e) => setRepeat(e.target.value)}
        />
      </label>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <div className="row">
        <button type="submit" disabled={busy}>
          {busy ? 'Saving...' : 'Change password'}
        </button>
        {!forced && (
          <button type="button" className="secondary" onClick={onCancel} disabled={busy}>
            Cancel
          </button>
        )}
      </div>
      <p className="muted hint">Changing the password signs out every other browser.</p>
    </form>
  )
}

function Home({
  session,
  onChangePassword,
  onSignedOut,
  onSessionEnded,
}: {
  session: SessionInfo
  onChangePassword: () => void
  onSignedOut: () => void
  onSessionEnded: () => void
}) {
  const [info, setInfo] = useState<ServerInfo | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api.serverInfo().then(setInfo, (e: unknown) => {
      if (e instanceof ApiFailure && e.code === 'unauthorized') onSessionEnded()
      else setError(errorText(e))
    })
  }, [onSessionEnded])

  const signOut = async () => {
    try {
      await api.signOut()
      onSignedOut()
    } catch (e) {
      setError(errorText(e))
    }
  }

  return (
    <section>
      <p>
        Signed in as <code>{session.username}</code>
        {info && (
          <>
            {' '}
            on <code>{info.hostname}</code>, server version {info.version}
          </>
        )}
        .
      </p>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <p className="muted">The desktop is not built yet.</p>
      <div className="row">
        <button type="button" className="secondary" onClick={onChangePassword}>
          Change password
        </button>
        <button type="button" onClick={signOut}>
          Sign out
        </button>
      </div>
    </section>
  )
}
