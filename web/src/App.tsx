// Placeholder until the Claude Design handoff arrives (docs/design/).
// It only proves the wiring: UI → API → generated types.
import { useEffect, useState } from 'react'
import { api } from './api/client'
import type { ServerInfo } from './api/generated/ServerInfo'

export default function App() {
  const [info, setInfo] = useState<ServerInfo | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    api.serverInfo().then(setInfo, (e: Error) => setError(e.message))
  }, [])

  return (
    <main className="placeholder">
      <h1>Webdesk</h1>
      {info && (
        <p>
          Connected to <code>{info.hostname}</code>, server v{info.version}.
        </p>
      )}
      {error && <p className="error">Can't reach webdeskd: {error}</p>}
      {!info && !error && <p>Connecting…</p>}
      <p className="muted">The desktop UI comes next, from the Claude Design handoff.</p>
    </main>
  )
}
