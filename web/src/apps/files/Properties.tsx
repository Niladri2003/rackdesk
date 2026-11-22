// The properties of 1 entry: what it is, who owns it, and its permission
// bits. The owner and the bits can be changed here.
//
// A change is tried with the rights of the server first. If the system
// refuses and the server has a root program, the dialog offers to apply the
// change as administrator.
import { CircleAlert, ShieldAlert } from 'lucide-react'
import { useEffect, useState } from 'react'
import type { SubmitEvent } from 'react'
import { api, ApiFailure, elevatedFileUrl } from '../../api/client'
import type { EntryDetails } from '../../api/generated/EntryDetails'
import { errorText, t } from '../../i18n'
import { folderChanged } from './changes'
import { kindOf } from './kinds'
import { formatSize, formatTime, parentPath } from './paths'

const f = t.files
const WHO = ['owner', 'group', 'others'] as const
const BITS = ['read', 'write', 'execute'] as const

/** The bit for 1 box of the grid: owner-read is 0o400, others-execute 0o001. */
const bitOf = (who: number, bit: number) => 1 << ((2 - who) * 3 + (2 - bit))

export function Properties({ path, close }: { path: string; close: () => void }) {
  const [details, setDetails] = useState<EntryDetails | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [mode, setMode] = useState(0)
  const [owner, setOwner] = useState('')
  const [group, setGroup] = useState('')
  const [busy, setBusy] = useState(false)
  // True after the server refused the change with its own rights.
  const [refused, setRefused] = useState(false)

  useEffect(() => {
    let live = true
    api.fs.properties(path).then(
      (d) => {
        if (!live) return
        setDetails(d)
        setMode((d.entry.mode ?? 0) & 0o777)
        setOwner(d.owner ?? String(d.uid ?? ''))
        setGroup(d.group ?? String(d.gid ?? ''))
      },
      (e: unknown) => live && setError(errorText(e)),
    )
    return () => {
      live = false
    }
  }, [path])

  if (!details) {
    return error ? (
      <div className="alert" role="alert">
        <CircleAlert size={15} aria-hidden="true" />
        <span>{error}</span>
      </div>
    ) : (
      <p className="muted" role="status">
        {f.loading}
      </p>
    )
  }

  const { entry } = details
  const kind = kindOf(entry)
  const oldMode = (entry.mode ?? 0) & 0o777
  const oldOwner = details.owner ?? String(details.uid ?? '')
  const oldGroup = details.group ?? String(details.gid ?? '')
  const ownerChanged = owner.trim() !== oldOwner
  const groupChanged = group.trim() !== oldGroup
  const changed = mode !== oldMode || ownerChanged || groupChanged
  // A link has no bits of its own to set.
  const isLink = entry.kind === 'symlink'

  const apply = async (elevate: boolean) => {
    setBusy(true)
    setError(null)
    try {
      if (mode !== oldMode) await api.fs.chmod(path, mode, elevate)
      if (ownerChanged || groupChanged) await api.fs.chown(path, ownerChanged ? owner.trim() : null, groupChanged ? group.trim() : null, elevate)
      folderChanged(parentPath(path))
      close()
    } catch (e) {
      if (e instanceof ApiFailure && e.code === 'permission_denied' && !elevate) setRefused(true)
      else setError(errorText(e))
    } finally {
      setBusy(false)
    }
  }
  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    void apply(false)
  }

  return (
    <form className="props" onSubmit={submit}>
      <dl className="props-facts">
        <div>
          <dt>{f.propKind}</dt>
          <dd>{kind.label}</dd>
        </div>
        {kind.open !== 'folder' && (
          <div>
            <dt>{f.propSize}</dt>
            <dd>
              {formatSize(entry.size)} <span className="muted">({f.bytes(entry.size)})</span>
            </dd>
          </div>
        )}
        <div>
          <dt>{f.propModified}</dt>
          <dd>{formatTime(entry.modified)}</dd>
        </div>
        <div>
          <dt>{f.propLocation}</dt>
          <dd className="mono props-path" title={path}>
            {parentPath(path)}
          </dd>
        </div>
      </dl>

      <div className="props-owners">
        <label className="field">
          <span>{f.propOwner}</span>
          <input className="input" spellCheck={false} autoComplete="off" value={owner} onChange={(e) => setOwner(e.target.value)} />
        </label>
        <label className="field">
          <span>{f.propGroup}</span>
          <input className="input" spellCheck={false} autoComplete="off" value={group} onChange={(e) => setGroup(e.target.value)} />
        </label>
      </div>

      {!isLink && (
        <fieldset className="props-grid">
          <legend>
            {f.propPermissions} <span className="mono muted">{f.propMode(mode.toString(8).padStart(3, '0'))}</span>
          </legend>
          <span />
          {BITS.map((bit) => (
            <span key={bit} className="props-head" aria-hidden="true">
              {f.propBits[bit]}
            </span>
          ))}
          {WHO.map((who, w) => (
            <div key={who} className="props-row">
              <span aria-hidden="true">{f.propWho[who]}</span>
              {BITS.map((bit, b) => (
                <input
                  key={bit}
                  type="checkbox"
                  aria-label={f.propBitLabel(f.propWho[who], f.propBits[bit])}
                  checked={(mode & bitOf(w, b)) !== 0}
                  onChange={(e) => setMode((m) => (e.target.checked ? m | bitOf(w, b) : m & ~bitOf(w, b)))}
                />
              ))}
            </div>
          ))}
        </fieldset>
      )}

      {error && (
        <div className="alert" role="alert">
          <CircleAlert size={15} aria-hidden="true" />
          <span>{error}</span>
        </div>
      )}
      {refused && (
        <div className="props-admin" role="alert">
          <ShieldAlert size={16} aria-hidden="true" />
          <span>{details.can_elevate ? f.needsAdmin : `${f.needsAdmin} ${t.errors.no_root_program}`}</span>
          {details.can_elevate && (
            <button type="button" className="btn btn-small btn-danger" disabled={busy} onClick={() => void apply(true)}>
              {f.applyAsAdmin}
            </button>
          )}
        </div>
      )}

      <div className="dialog-actions">
        {details.can_elevate && (entry.kind === 'file' || entry.target === 'file') && (
          <a className="btn props-left" href={elevatedFileUrl(path)} download={entry.name}>
            {f.downloadAsAdmin}
          </a>
        )}
        <button type="button" className="btn" onClick={close}>
          {t.common.close}
        </button>
        <button type="submit" className="btn btn-primary" disabled={!changed || busy}>
          {busy ? f.applying : f.apply}
        </button>
      </div>
    </form>
  )
}
