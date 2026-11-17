// Shows 1 file: an image, a video, an audio file or text. The server decides
// which types the browser may show in the page. Text is drawn as text nodes.
import { useVirtualizer } from '@tanstack/react-virtual'
import { CircleAlert, Download, ExternalLink, FileQuestion, FolderOpen } from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { ApiFailure, fileUrl, readFileStart } from '../../api/client'
import { errorText, t } from '../../i18n'
import { useShell } from '../../shell/context'
import { kindOfName } from '../files/kinds'
import type { Syntax } from '../files/kinds'
import { baseName, formatSize, parentPath } from '../files/paths'
import type { AppProps } from '../registry'
import { highlightLine } from './highlight'
import { ImagePreview } from './ImagePreview'

/** The text preview reads at most this much of a file. */
const TEXT_LIMIT = 256 * 1024
const LINE_HEIGHT = 20
const p = t.preview

export function Preview({ params }: AppProps) {
  const shell = useShell()
  const path = params.path ?? '/'
  const name = baseName(path)
  const kind = kindOfName(name)
  const [failed, setFailed] = useState(false)
  const [note, setNote] = useState('')

  const fallback = (text: string) => (
    <div className="empty-state">
      <FileQuestion size={24} aria-hidden="true" />
      <p className="muted">{text}</p>
    </div>
  )

  let body: React.ReactNode
  if (failed) body = fallback(p.noPreview)
  else if (kind.open === 'image') body = <ImagePreview src={fileUrl(path, true)} name={name} onError={() => setFailed(true)} />
  else if (kind.open === 'video') body = <video className="preview-media" src={fileUrl(path, true)} controls aria-label={name} onError={() => setFailed(true)} />
  else if (kind.open === 'audio') body = <audio className="preview-audio" src={fileUrl(path, true)} controls aria-label={name} onError={() => setFailed(true)} />
  else if (kind.open === 'text') body = <TextPreview path={path} name={name} syntax={kind.syntax} onNote={setNote} />
  else if (kind.open === 'pdf') body = fallback(p.pdfBody)
  else body = fallback(p.noPreview)

  return (
    <div className="preview">
      <div className="preview-body" data-kind={kind.open}>
        {body}
      </div>
      <div className="files-foot">
        <span className="files-foot-name mono" title={path}>
          {path}
        </span>
        <span>{note}</span>
        <span className="win-spacer" />
        <button type="button" className="btn btn-small" onClick={() => shell.openApp('files', { path: parentPath(path) })}>
          <FolderOpen size={14} aria-hidden="true" />
          {p.showInFiles}
        </button>
        {kind.open === 'pdf' && (
          <a className="btn btn-small" href={fileUrl(path, true)} target="_blank" rel="noopener noreferrer">
            <ExternalLink size={14} aria-hidden="true" />
            {p.openInTab}
          </a>
        )}
        <a className="btn btn-small" href={fileUrl(path)} download={name}>
          <Download size={14} aria-hidden="true" />
          {p.download}
        </a>
      </div>
    </div>
  )
}

type TextState = { status: 'loading' } | { status: 'error'; error: unknown } | { status: 'binary' } | { status: 'ready'; lines: string[] }

function TextPreview({ path, name, syntax, onNote }: { path: string; name: string; syntax: Syntax; onNote: (note: string) => void }) {
  const { sessionEnded } = useShell()
  const [state, setState] = useState<TextState>({ status: 'loading' })
  const scrollRef = useRef<HTMLDivElement>(null)

  // Read once, when the window opens.
  useEffect(() => {
    const abort = new AbortController()
    readFileStart(path, TEXT_LIMIT, abort.signal).then(
      ({ bytes, total }) => {
        // A NUL byte in the first 8 KiB means the file is not text.
        if (bytes.subarray(0, 8192).includes(0)) {
          setState({ status: 'binary' })
          return
        }
        const lines = new TextDecoder('utf-8').decode(bytes).split('\n')
        if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop()
        setState({ status: 'ready', lines })
        onNote(total > bytes.length ? p.truncated(formatSize(bytes.length), formatSize(total)) : p.lines(lines.length))
      },
      (error: unknown) => {
        if (abort.signal.aborted) return
        if (error instanceof ApiFailure && error.code === 'unauthorized') sessionEnded()
        else setState({ status: 'error', error })
      },
    )
    return () => abort.abort()
  }, [path, onNote, sessionEnded])

  const lines = state.status === 'ready' ? state.lines : EMPTY
  // The React Compiler is not used here, so its warning about this library
  // does not apply.
  // oxlint-disable-next-line react-hooks/incompatible-library
  const virtual = useVirtualizer({
    count: lines.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => LINE_HEIGHT,
    overscan: 20,
  })
  const gutter = useMemo(() => `${String(lines.length).length + 2}ch`, [lines.length])

  if (state.status === 'loading') {
    return (
      <div className="empty-state" role="status">
        <span className="spinner" aria-hidden="true" />
        <p className="muted">{p.loading}</p>
      </div>
    )
  }
  if (state.status === 'error' || state.status === 'binary') {
    return (
      <div className="empty-state" role={state.status === 'error' ? 'alert' : undefined}>
        <CircleAlert size={24} aria-hidden="true" />
        <p className="muted">{state.status === 'error' ? errorText(state.error) : p.binary}</p>
      </div>
    )
  }
  return (
    <div ref={scrollRef} className="preview-text" tabIndex={0} role="region" aria-label={p.textLabel(name)}>
      <div className="files-canvas" style={{ height: virtual.getTotalSize() }}>
        {virtual.getVirtualItems().map((item) => (
          <div key={item.index} className="preview-line" style={{ top: item.start, height: LINE_HEIGHT }}>
            <span className="preview-number" style={{ width: gutter }} aria-hidden="true">
              {item.index + 1}
            </span>
            <code>
              {highlightLine(lines[item.index], syntax).map((piece, i) =>
                piece.kind ? (
                  <span key={i} className={piece.kind}>
                    {piece.text}
                  </span>
                ) : (
                  piece.text
                ),
              )}
            </code>
          </div>
        ))}
      </div>
    </div>
  )
}

const EMPTY: string[] = []
