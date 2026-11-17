// The panel in the bottom right corner that lists uploads with their
// progress.
import { ArrowUpDown, ChevronDown, ChevronUp, CircleCheck, Pause, Play, RotateCw, X } from 'lucide-react'
import { memo, useState } from 'react'
import { formatSize } from '../apps/files/paths'
import { t } from '../i18n'
import { cancelTransfer, clearFinished, pauseTransfer, resumeTransfer, useTransfers } from './transfers'

export const TransferPanel = memo(function TransferPanel() {
  const list = useTransfers()
  const [open, setOpen] = useState(true)
  if (list.length === 0) return null
  const x = t.transfers
  const active = list.filter((item) => item.state !== 'done').length

  return (
    <section className="panel transfers" aria-label={x.title}>
      <header>
        <ArrowUpDown size={16} aria-hidden="true" />
        <span>
          <b>{x.title}</b>
          <span className="muted" role="status">
            {active ? x.active(active) : x.allDone(list.length)}
          </span>
        </span>
        <button type="button" className="icon-btn" aria-label={open ? x.collapse : x.expand} aria-expanded={open} onClick={() => setOpen((v) => !v)}>
          {open ? <ChevronDown size={16} aria-hidden="true" /> : <ChevronUp size={16} aria-hidden="true" />}
        </button>
        <button type="button" className="icon-btn" aria-label={x.clear} title={x.clear} onClick={clearFinished}>
          <X size={16} aria-hidden="true" />
        </button>
      </header>
      {open && (
        <ul>
          {list.map((item) => {
            const percent = item.size ? Math.floor((item.sent / item.size) * 100) : 100
            const detail =
              item.state === 'failed'
                ? (item.error ?? x.failed)
                : item.state === 'paused'
                  ? x.paused
                  : item.state === 'waiting'
                    ? x.waiting
                    : item.state === 'done'
                      ? x.doneIn(item.dir)
                      : x.progress(formatSize(item.sent), formatSize(item.size))
            return (
              <li key={item.id} data-state={item.state}>
                <div>
                  <span className="transfer-name" title={item.name}>
                    {item.name}
                  </span>
                  <span className="mono">{percent}%</span>
                </div>
                <div className="transfer-bar" role="progressbar" aria-label={item.name} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}>
                  <span style={{ width: `${percent}%` }} />
                </div>
                <div>
                  <span className="transfer-detail">{detail}</span>
                  {item.state === 'sending' && (
                    <button type="button" className="icon-btn" aria-label={x.pause(item.name)} title={x.pauseTitle} onClick={() => pauseTransfer(item.id)}>
                      <Pause size={14} aria-hidden="true" />
                    </button>
                  )}
                  {item.state === 'paused' && (
                    <button type="button" className="icon-btn" aria-label={x.resume(item.name)} title={x.resumeTitle} onClick={() => resumeTransfer(item.id)}>
                      <Play size={14} aria-hidden="true" />
                    </button>
                  )}
                  {item.state === 'failed' && (
                    <button type="button" className="icon-btn" aria-label={x.retry(item.name)} title={x.retryTitle} onClick={() => resumeTransfer(item.id)}>
                      <RotateCw size={14} aria-hidden="true" />
                    </button>
                  )}
                  {item.state === 'done' ? (
                    <CircleCheck size={16} data-kind="success" aria-hidden="true" />
                  ) : (
                    <button type="button" className="icon-btn" aria-label={x.cancel(item.name)} title={x.cancelTitle} onClick={() => cancelTransfer(item.id)}>
                      <X size={14} aria-hidden="true" />
                    </button>
                  )}
                </div>
              </li>
            )
          })}
        </ul>
      )}
    </section>
  )
})
