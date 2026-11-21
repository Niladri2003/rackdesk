// The audit log, as a table. It is read when this section opens and when
// the user asks for a refresh. Nothing reloads it by itself.
import { CircleAlert, RefreshCw } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { api, ApiFailure } from '../../api/client'
import type { AuditEntry } from '../../api/generated/AuditEntry'
import { errorText, t } from '../../i18n'
import { useShell } from '../../shell/context'

const a = t.settings.activity
const timeFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'medium' })

function formatTime(iso: string): string {
  const date = new Date(iso)
  return Number.isNaN(date.getTime()) ? iso : timeFormat.format(date)
}

export function Activity() {
  const { sessionEnded } = useShell()
  const [entries, setEntries] = useState<AuditEntry[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [onlyFailed, setOnlyFailed] = useState(false)
  const [filter, setFilter] = useState('')
  const [version, setVersion] = useState(0)

  useEffect(() => {
    let live = true
    api.audit().then(
      (list) => {
        if (!live) return
        setEntries(list)
        setError(null)
      },
      (e: unknown) => {
        if (!live) return
        if (e instanceof ApiFailure && e.code === 'unauthorized') sessionEnded()
        else setError(errorText(e))
      },
    )
    return () => {
      live = false
    }
  }, [version, sessionEnded])
  const refresh = useCallback(() => setVersion((v) => v + 1), [])

  const needle = filter.trim().toLowerCase()
  const shown = (entries ?? []).filter(
    (e) => (!onlyFailed || e.result !== 'ok') && (!needle || `${e.action} ${e.target} ${e.ip} ${e.user} ${e.result}`.toLowerCase().includes(needle)),
  )

  return (
    <>
      <header className="settings-heading">
        <h2>{a.name}</h2>
        <p className="muted">{a.lead}</p>
      </header>
      <div className="activity-bar">
        <input
          className="input"
          type="search"
          aria-label={a.filter}
          placeholder={a.filter}
          spellCheck={false}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <label className="activity-check">
          <input type="checkbox" checked={onlyFailed} onChange={(e) => setOnlyFailed(e.target.checked)} />
          {a.onlyFailed}
        </label>
        <button type="button" className="btn" onClick={refresh}>
          <RefreshCw size={14} aria-hidden="true" />
          {a.refresh}
        </button>
      </div>
      {error && (
        <div className="alert" role="alert">
          <CircleAlert size={15} aria-hidden="true" />
          <span>{error}</span>
        </div>
      )}
      {entries === null && !error ? (
        <p className="muted" role="status">
          {t.common.connecting}
        </p>
      ) : shown.length === 0 ? (
        <p className="muted">{entries?.length ? a.noMatch : a.empty}</p>
      ) : (
        <>
          <div className="activity-scroll" tabIndex={0} role="region" aria-label={a.table}>
            <table className="activity">
              <thead>
                <tr>
                  <th scope="col">{a.colTime}</th>
                  <th scope="col">{a.colAction}</th>
                  <th scope="col">{a.colTarget}</th>
                  <th scope="col">{a.colFrom}</th>
                  <th scope="col">{a.colResult}</th>
                </tr>
              </thead>
              <tbody>
                {shown.map((e, i) => (
                  <tr key={`${e.time}-${i}`} data-failed={e.result !== 'ok'}>
                    <td>{formatTime(e.time)}</td>
                    <td className="mono">{e.action}</td>
                    <td className="mono activity-target" title={e.target}>
                      {e.target}
                    </td>
                    <td className="mono">{e.ip}</td>
                    <td className="activity-result">{e.result === 'ok' ? a.ok : e.result}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p className="muted small" role="status">
            {a.shown(shown.length)}
          </p>
        </>
      )}
    </>
  )
}
