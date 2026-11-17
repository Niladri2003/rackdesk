// Window content for an app that is planned but not built.
import { Hammer } from 'lucide-react'
import { t } from '../i18n'
import type { AppProps } from './registry'

export function Placeholder({ app }: AppProps) {
  return (
    <div className="empty-state">
      <Hammer size={24} aria-hidden="true" />
      <b>{t.apps.notBuiltTitle(app.name)}</b>
      <p className="muted">{t.apps.notBuiltBody}</p>
    </div>
  )
}
