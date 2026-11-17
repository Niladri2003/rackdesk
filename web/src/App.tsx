// The top of the UI: which page to show for the current session state.
import { useCallback, useEffect, useState } from 'react'
import { api, ApiFailure } from './api/client'
import type { SessionInfo } from './api/generated/SessionInfo'
import { SetPassword } from './auth/SetPassword'
import { SignIn } from './auth/SignIn'
import { errorText, t } from './i18n'
import { Desktop } from './shell/Desktop'
import { notify } from './shell/notify'
import { useApplyTokens } from './theme/prefs'

type View =
  | { name: 'loading' }
  | { name: 'unreachable'; message: string }
  | { name: 'signed-out'; notice?: string }
  // `password` is the one typed at sign-in, kept only until the first
  // password is set.
  | { name: 'signed-in'; session: SessionInfo; password: string | null }

export default function App() {
  useApplyTokens()
  const [view, setView] = useState<View>({ name: 'loading' })

  const load = useCallback(() => {
    api.session().then(
      (session) => setView({ name: 'signed-in', session, password: null }),
      (e: unknown) => {
        if (e instanceof ApiFailure && e.code === 'unauthorized') setView({ name: 'signed-out' })
        else setView({ name: 'unreachable', message: errorText(e) })
      },
    )
  }, [])
  useEffect(load, [load])

  const signedOut = useCallback(() => setView({ name: 'signed-out' }), [])
  const sessionEnded = useCallback(() => setView({ name: 'signed-out', notice: t.errors.unauthorized }), [])

  if (view.name === 'loading' || view.name === 'unreachable') {
    return (
      <div className="wall">
        <div className="auth">
          {view.name === 'loading' ? (
            <p className="auth-status" role="status">
              <span className="spinner" aria-hidden="true" />
              {t.common.connecting}
            </p>
          ) : (
            <div className="auth-card">
              <p role="alert">{view.message}</p>
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => {
                  setView({ name: 'loading' })
                  load()
                }}
              >
                {t.common.retry}
              </button>
            </div>
          )}
        </div>
      </div>
    )
  }

  if (view.name === 'signed-out') {
    return (
      <div className="wall">
        <SignIn
          notice={view.notice}
          onDone={(session, password) => setView({ name: 'signed-in', session, password })}
        />
      </div>
    )
  }

  if (view.session.must_change_password) {
    return (
      <div className="wall">
        <SetPassword
          session={view.session}
          currentPassword={view.password}
          onDone={(session) => {
            setView({ name: 'signed-in', session, password: null })
            notify('success', t.setPassword.done)
          }}
          onSessionEnded={sessionEnded}
        />
      </div>
    )
  }

  return <Desktop session={view.session} onSignedOut={signedOut} onSessionEnded={sessionEnded} />
}
