// The Trash, shown in the file browser in place of a folder.
import { CircleAlert, RotateCcw, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { api, ApiFailure } from '../../api/client'
import type { TrashItem } from '../../api/generated/TrashItem'
import { errorText, t } from '../../i18n'
import { useShell } from '../../shell/context'
import { deleteFromTrash, emptyTrash, restoreFromTrash } from './actions'
import { TRASH, useFolderChanges } from './changes'
import { kindOf } from './kinds'
import { formatTime, parentPath } from './paths'

const f = t.files

export function TrashView() {
  const { sessionEnded } = useShell()
  const [items, setItems] = useState<TrashItem[] | null>(null)
  const [error, setError] = useState<unknown>(null)
  const [version, setVersion] = useState(0)

  // Loaded when the view opens, and again after a change to the trash.
  useEffect(() => {
    let live = true
    api.fs.trashList().then(
      (list) => {
        if (!live) return
        setItems(list)
        setError(null)
      },
      (e: unknown) => {
        if (!live) return
        if (e instanceof ApiFailure && e.code === 'unauthorized') sessionEnded()
        else setError(e)
      },
    )
    return () => {
      live = false
    }
  }, [version, sessionEnded])
  const reload = useCallback(() => setVersion((v) => v + 1), [])
  useFolderChanges(TRASH, reload)

  if (error) {
    return (
      <div className="files-scroll">
        <div className="empty-state" role="alert">
          <CircleAlert size={24} aria-hidden="true" />
          <b>{errorText(error)}</b>
          <button type="button" className="btn" onClick={reload}>
            {t.common.retry}
          </button>
        </div>
      </div>
    )
  }
  if (items === null) {
    return (
      <div className="files-scroll">
        <div className="empty-state" role="status">
          <span className="spinner" aria-hidden="true" />
          <p className="muted">{f.loading}</p>
        </div>
      </div>
    )
  }

  return (
    <>
      <div className="files-scroll trash">
        {items.length === 0 ? (
          <div className="empty-state">
            <Trash2 size={24} aria-hidden="true" />
            <p className="muted">{f.trashEmpty}</p>
          </div>
        ) : (
          <ul aria-label={f.trashList}>
            {items.map((item) => {
              const kind = kindOf(item.entry)
              const Icon = kind.icon
              return (
                <li key={item.id} className="trash-row">
                  <Icon size={16} style={{ color: kind.tint }} aria-hidden="true" />
                  <span className="trash-name">
                    <span title={item.entry.name}>{item.entry.name}</span>
                    <span className="mono" title={item.original_path}>
                      {f.trashFrom} {parentPath(item.original_path)}
                    </span>
                  </span>
                  <span className="files-time">{formatTime(item.deleted_at)}</span>
                  <button type="button" className="btn btn-small" aria-label={f.restoreOf(item.entry.name)} onClick={() => void restoreFromTrash(item.id, item.entry.name)}>
                    <RotateCcw size={14} aria-hidden="true" />
                    {f.restore}
                  </button>
                  <button
                    type="button"
                    className="icon-btn danger"
                    aria-label={f.deleteOf(item.entry.name)}
                    title={f.deleteForever}
                    onClick={() => void deleteFromTrash(item.id, item.entry.name)}
                  >
                    <Trash2 size={14} aria-hidden="true" />
                  </button>
                </li>
              )
            })}
          </ul>
        )}
      </div>
      <div className="files-foot">
        <span role="status">{f.items(items.length)}</span>
        <span className="win-spacer" />
        {items.length > 0 && (
          <button type="button" className="btn btn-small btn-danger" onClick={() => void emptyTrash(items.length)}>
            <Trash2 size={14} aria-hidden="true" />
            {f.emptyTrash}
          </button>
        )}
      </div>
    </>
  )
}
