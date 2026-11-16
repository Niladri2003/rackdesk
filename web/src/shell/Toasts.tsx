// Short messages in the top right corner, and the banner for a lost
// connection.
import { WifiOff, X } from 'lucide-react'
import { memo } from 'react'
import { t } from '../i18n'
import { retryNow, useConnection } from './connection'
import { NOTICE_ICON } from './icons'
import { dismissToast, useNotices } from './notify'

export const Toasts = memo(function Toasts() {
  const { toasts } = useNotices()
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((n) => {
        const Icon = NOTICE_ICON[n.kind]
        return (
          <div key={n.id} className="panel toast" role="status">
            <Icon size={17} data-kind={n.kind} aria-hidden="true" />
            <span>{n.text}</span>
            <button type="button" className="icon-btn" aria-label={t.common.dismiss} onClick={() => dismissToast(n.id)}>
              <X size={14} aria-hidden="true" />
            </button>
          </div>
        )
      })}
    </div>
  )
})

export const ConnectionBanner = memo(function ConnectionBanner() {
  const { online, retryIn } = useConnection()
  if (online) return null
  return (
    <div className="banner" role="status">
      <WifiOff size={16} aria-hidden="true" />
      <b>{t.connection.lost}</b>
      <span>{retryIn > 0 ? t.connection.retryIn(retryIn) : t.connection.retrying}</span>
      <span className="spinner" aria-hidden="true" />
      <button type="button" onClick={() => void retryNow()}>
        {t.common.retry}
      </button>
    </div>
  )
})
