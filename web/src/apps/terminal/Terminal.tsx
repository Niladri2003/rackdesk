// The terminal: a shell on the server, drawn with xterm.js. Bytes travel
// over a WebSocket to /api/v1/term.
//
// The terminal library is loaded when the first terminal window opens, so
// it is not part of the first page load.
//
// Messages to the server: a binary message starts with a type byte. 0: the
// rest is input. 1: the next 4 bytes are the size, columns then rows. The
// text "close" ends the shell. From the server: binary output, and 2 JSON
// texts, `ready` with the id of the terminal and `exit`.
import { CircleAlert } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { api, ApiFailure } from '../../api/client'
import { errorText, t } from '../../i18n'
import { useShell } from '../../shell/context'
import type { AppProps } from '../registry'

/// A lost connection is tried again for this long. The server keeps the
/// shell for 60 s.
const RECONNECT_FOR_MS = 55_000
const RECONNECT_EVERY_MS = 2000
const x = t.terminal

type Status = 'starting' | 'open' | 'reconnecting' | 'ended' | 'off' | 'failed'

/// Reads a design token. The terminal library needs colour values, not
/// names of variables.
/// Line breaks in the value are turned into spaces, so a font list written
/// on several lines in the stylesheet is 1 line here.
const token = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).replace(/\s+/g, ' ').trim()

export function Terminal({ params, setParams }: AppProps) {
  const { sessionEnded } = useShell()
  const [host, setHost] = useState<HTMLDivElement | null>(null)
  const [status, setStatus] = useState<Status>('starting')
  const [error, setError] = useState<string | null>(null)
  // Raised to start a new shell after the old one has ended.
  const [run, setRun] = useState(0)
  // Read once per run. A change of these must not restart the shell.
  const start = useRef<{ id?: string; cwd?: string; setParams: AppProps['setParams'] }>({ id: params.id || undefined, cwd: params.cwd, setParams })
  useEffect(() => {
    start.current.setParams = setParams
  })

  useEffect(() => {
    if (!host) return
    let disposed = false
    let socket: WebSocket | null = null
    let cleanup = () => {}

    void (async () => {
      let info
      try {
        info = await api.serverInfo()
      } catch (e) {
        if (disposed) return
        if (e instanceof ApiFailure && e.code === 'unauthorized') return sessionEnded()
        setError(errorText(e))
        return setStatus('failed')
      }
      if (disposed) return
      if (!info.terminal) return setStatus('off')

      const [{ Terminal: Xterm }, { FitAddon }] = await Promise.all([import('@xterm/xterm'), import('@xterm/addon-fit'), import('@xterm/xterm/css/xterm.css')])
      if (disposed) return
      const term = new Xterm({
        fontFamily: token('--font-term'),
        fontSize: 13,
        lineHeight: 1.2,
        cursorBlink: true,
        scrollback: 5000,
        // Keeps a text copy of the screen in a form that screen readers
        // announce.
        screenReaderMode: true,
        theme: {
          background: token('--term-bg'),
          foreground: token('--term-fg'),
          cursor: token('--term-fg'),
          selectionBackground: token('--term-selection'),
        },
      })
      const fit = new FitAddon()
      term.loadAddon(fit)
      term.open(host)
      fit.fit()
      term.textarea?.setAttribute('aria-label', x.inputLabel)

      const encoder = new TextEncoder()
      const sendInput = (data: string) => {
        if (socket?.readyState !== WebSocket.OPEN) return
        const bytes = encoder.encode(data)
        const message = new Uint8Array(bytes.length + 1)
        message.set(bytes, 1)
        socket.send(message)
      }
      const sendSize = () => {
        if (socket?.readyState !== WebSocket.OPEN) return
        socket.send(new Uint8Array([1, term.cols >> 8, term.cols & 255, term.rows >> 8, term.rows & 255]))
      }
      term.onData(sendInput)
      term.onResize(sendSize)

      let id = start.current.id
      let ended = false
      let lostAt = 0
      let retry: ReturnType<typeof setTimeout> | undefined

      const connect = () => {
        const query = new URLSearchParams({ cols: String(term.cols), rows: String(term.rows) })
        if (id) query.set('id', id)
        else if (start.current.cwd) query.set('cwd', start.current.cwd)
        const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws'
        const ws = new WebSocket(`${scheme}://${window.location.host}/api/v1/term?${query}`)
        ws.binaryType = 'arraybuffer'
        socket = ws
        ws.onmessage = (event) => {
          if (typeof event.data !== 'string') return term.write(new Uint8Array(event.data as ArrayBuffer))
          const message = JSON.parse(event.data) as { type: string; id?: string }
          if (message.type === 'ready' && message.id) {
            // After a lost connection the server sends the last output
            // again. Start from an empty screen, so it is not shown twice.
            if (lostAt) term.reset()
            lostAt = 0
            id = message.id
            // Kept with the window, so a reload finds the same shell.
            start.current.setParams({ id: message.id })
            setStatus('open')
            sendSize()
          } else if (message.type === 'exit') {
            ended = true
            setStatus('ended')
          }
        }
        ws.onclose = () => {
          if (disposed || ended) return
          // The connection is gone but the shell may still run. Try to get
          // back to it until the server would have stopped it.
          lostAt ||= Date.now()
          if (Date.now() - lostAt > RECONNECT_FOR_MS) {
            ended = true
            return setStatus('ended')
          }
          setStatus('reconnecting')
          retry = setTimeout(connect, RECONNECT_EVERY_MS)
        }
      }
      connect()

      const observer = new ResizeObserver(() => fit.fit())
      observer.observe(host)
      // A click on the title bar or the taskbar focuses the window. Typing
      // must then go to the shell.
      const win = host.closest('.win')
      const focus = () => term.focus()
      win?.addEventListener('focus', focus)
      // The library takes a moment to load. If the user has moved to
      // another window by now, this one must not pull the focus back.
      if (win?.getAttribute('data-focused') === 'true') term.focus()

      cleanup = () => {
        clearTimeout(retry)
        observer.disconnect()
        win?.removeEventListener('focus', focus)
        // The window was closed: stop the shell now, not after 60 s.
        if (socket?.readyState === WebSocket.OPEN) socket.send('close')
        socket?.close()
        term.dispose()
      }
      if (disposed) cleanup()
    })()

    return () => {
      disposed = true
      cleanup()
    }
  }, [host, run, sessionEnded])

  const restart = () => {
    start.current.id = undefined
    setParams({ id: '' })
    setError(null)
    setStatus('starting')
    setRun((n) => n + 1)
  }

  const overlay =
    status === 'off' ? x.off : status === 'failed' ? (error ?? x.failed) : status === 'ended' ? x.ended : status === 'reconnecting' ? x.reconnecting : null

  return (
    <div className="terminal">
      {/* A new element per run, so a new shell starts on an empty screen. */}
      <div key={run} ref={setHost} className="terminal-host" />
      {status === 'starting' && (
        <div className="terminal-note" role="status">
          <span className="spinner" aria-hidden="true" />
          {x.starting}
        </div>
      )}
      {overlay && (
        <div className="terminal-note" role={status === 'reconnecting' ? 'status' : 'alert'}>
          {status === 'reconnecting' ? <span className="spinner" aria-hidden="true" /> : <CircleAlert size={16} aria-hidden="true" />}
          <span>{overlay}</span>
          {(status === 'ended' || status === 'failed') && (
            <button type="button" className="btn btn-small" onClick={restart}>
              {x.restart}
            </button>
          )}
        </div>
      )}
    </div>
  )
}
