// A text editor for 1 file. It reads the whole file, so it has a size
// limit. A save sends the version that was read, and the server refuses the
// save if someone else changed the file since.
//
// The editor library (CodeMirror) is loaded when the first editor window
// opens, so it is not part of the first page load.
import { CircleAlert, Save } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { ApiFailure, readTextFile, saveTextFile } from '../../api/client'
import { errorText, t } from '../../i18n'
import { useShell } from '../../shell/context'
import { choose } from '../../shell/dialog'
import { folderChanged } from '../files/changes'
import { baseName, parentPath } from '../files/paths'
import type { AppProps } from '../registry'
import { createEditor } from './codemirror'
import type { EditorHandle } from './codemirror'

/** The same limit as the save route of the server. */
const MAX_BYTES = 2 * 1024 * 1024
const x = t.editor

type Status = { name: 'loading' } | { name: 'ready' } | { name: 'error'; error: unknown } | { name: 'not-text' } | { name: 'too-large' }

export function Editor({ params, setCloseGuard }: AppProps) {
  const shell = useShell()
  const { sessionEnded } = shell
  const path = params.path ?? '/'
  const name = baseName(path)

  const [host, setHost] = useState<HTMLDivElement | null>(null)
  const [status, setStatus] = useState<Status>({ name: 'loading' })
  const [dirty, setDirty] = useState(false)
  const [saving, setSaving] = useState(false)
  const [conflict, setConflict] = useState(false)
  const [message, setMessage] = useState<string | null>(null)
  const [lines, setLines] = useState(0)
  const [loadCount, setLoadCount] = useState(0)
  const editor = useRef<EditorHandle | null>(null)
  const etag = useRef('')
  const saveRef = useRef<(force?: boolean) => Promise<boolean>>(async () => false)

  // Load the file and make the editor. Runs again for "load the new version".
  useEffect(() => {
    if (!host) return
    const abort = new AbortController()
    let made: EditorHandle | null = null
    readTextFile(path, abort.signal)
      .then(async ({ bytes, etag: tag }) => {
        if (bytes.length > MAX_BYTES) return setStatus({ name: 'too-large' })
        let text: string
        try {
          // `fatal` refuses bytes that are not UTF-8. Editing such a file
          // as text would damage it on save.
          text = new TextDecoder('utf-8', { fatal: true }).decode(bytes)
        } catch {
          return setStatus({ name: 'not-text' })
        }
        if (text.includes('\0')) return setStatus({ name: 'not-text' })
        const handle = await createEditor({
          parent: host,
          text,
          fileName: name,
          label: x.textLabel(name),
          onChange: (lineCount) => {
            setDirty(true)
            setLines(lineCount)
          },
          onSave: () => void saveRef.current(),
        })
        if (abort.signal.aborted) return handle.destroy()
        made = handle
        editor.current = handle
        etag.current = tag
        setLines(handle.lineCount())
        setDirty(false)
        setConflict(false)
        setStatus({ name: 'ready' })
      })
      .catch((error: unknown) => {
        if (abort.signal.aborted) return
        if (error instanceof ApiFailure && error.code === 'unauthorized') sessionEnded()
        else setStatus({ name: 'error', error })
      })
    return () => {
      abort.abort()
      made?.destroy()
      editor.current = null
    }
  }, [host, path, name, loadCount, sessionEnded])

  const save = useCallback(
    async (force = false): Promise<boolean> => {
      const handle = editor.current
      if (!handle) return false
      setSaving(true)
      setMessage(null)
      try {
        etag.current = await saveTextFile(path, handle.text(), etag.current, force)
        setDirty(false)
        setConflict(false)
        folderChanged(parentPath(path))
        return true
      } catch (e) {
        if (e instanceof ApiFailure && e.code === 'changed_on_disk') setConflict(true)
        else if (e instanceof ApiFailure && e.code === 'unauthorized') sessionEnded()
        else setMessage(errorText(e))
        return false
      } finally {
        setSaving(false)
      }
    },
    [path, sessionEnded],
  )
  useEffect(() => {
    saveRef.current = save
  }, [save])

  // With unsaved changes, closing the window asks first.
  useEffect(() => {
    if (!dirty) {
      setCloseGuard(null)
      return
    }
    setCloseGuard(async () => {
      const answer = await choose<'save' | 'discard'>(x.closeTitle(name), x.closeBody, [
        { label: x.discard, value: 'discard', kind: 'danger' },
        { label: x.save, value: 'save', kind: 'primary' },
      ])
      if (answer === 'discard') return true
      if (answer === 'save') return saveRef.current()
      return false
    })
    return () => setCloseGuard(null)
  }, [dirty, name, setCloseGuard])

  const problem =
    status.name === 'error' ? errorText(status.error) : status.name === 'too-large' ? x.tooLarge : status.name === 'not-text' ? x.notText : null

  return (
    <div className="editor">
      {conflict && (
        <div className="editor-banner" role="alert">
          <CircleAlert size={16} aria-hidden="true" />
          <span>{x.conflict}</span>
          <button
            type="button"
            className="btn btn-small"
            onClick={() => {
              setStatus({ name: 'loading' })
              setLoadCount((n) => n + 1)
            }}
          >
            {x.reload}
          </button>
          <button type="button" className="btn btn-small btn-danger" onClick={() => void save(true)}>
            {x.overwrite}
          </button>
        </div>
      )}
      <div className="editor-body">
        {/* The editor is made inside this element. It stays on the page, so
            the library can keep it across a reload of the file. */}
        <div ref={setHost} className="editor-host" hidden={status.name !== 'ready'} />
        {status.name === 'loading' && (
          <div className="empty-state" role="status">
            <span className="spinner" aria-hidden="true" />
            <p className="muted">{x.loading}</p>
          </div>
        )}
        {problem && (
          <div className="empty-state" role="alert">
            <CircleAlert size={24} aria-hidden="true" />
            <p className="muted">{problem}</p>
            {status.name !== 'error' && (
              <button type="button" className="btn" onClick={() => shell.openApp('preview', { path })}>
                {x.openPreview}
              </button>
            )}
          </div>
        )}
      </div>
      <div className="files-foot">
        <span className="files-foot-name mono" title={path}>
          {path}
        </span>
        {status.name === 'ready' && <span>{x.lines(lines)}</span>}
        <span role="status" className={message ? 'editor-error' : undefined}>
          {message ?? (status.name !== 'ready' ? '' : saving ? x.saving : dirty ? x.unsaved : x.saved)}
        </span>
        <span className="win-spacer" />
        <button type="button" className="btn btn-small btn-primary" disabled={status.name !== 'ready' || !dirty || saving} onClick={() => void save()}>
          <Save size={14} aria-hidden="true" />
          {x.save}
        </button>
      </div>
    </div>
  )
}
