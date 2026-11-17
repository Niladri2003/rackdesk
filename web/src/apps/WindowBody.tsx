// Draws the content of a window, by content kind.
import { memo } from 'react'
import { t } from '../i18n'
import { countRender } from '../lib/renderCount'
import type { AppProps } from './registry'

/** Memoised: it draws again only when the app's own data changes, not when
 * its window moves, resizes or changes focus. */
export const WindowBody = memo(function WindowBody({ app, params, setParams, setCloseGuard }: AppProps) {
  countRender(`body:${app.id}`)
  const { content } = app
  if (content.kind === 'builtin') {
    const Component = content.component
    return <Component app={app} params={params} setParams={setParams} setCloseGuard={setCloseGuard} />
  }
  return (
    <div className="empty-state">
      <p className="muted">{content.kind === 'web' ? t.apps.webStub : t.apps.streamStub}</p>
    </div>
  )
})
