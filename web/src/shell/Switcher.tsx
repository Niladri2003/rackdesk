// The window switcher (Alt+`). It lists the open windows from most recently
// used to least. Releasing Alt switches to the selected one.
import { APPS } from '../apps/registry'
import { t } from '../i18n'
import type { Win } from './wm'

interface Props {
  /** Most recently used first. */
  wins: Win[]
  selected: number
  onPick: (index: number) => void
}

export function Switcher({ wins, selected, onPick }: Props) {
  return (
    <div className="switcher-scrim">
      <div className="switcher" role="listbox" aria-label={t.switcher.label} aria-activedescendant={`rd-switch-${selected}`} tabIndex={-1}>
        <div className="switcher-items">
          {wins.map((w, i) => {
            const app = APPS[w.app]
            const Icon = app.icon
            const detail = app.detail?.(w.params) ?? ''
            return (
              <div
                key={w.id}
                id={`rd-switch-${i}`}
                className="switcher-item"
                role="option"
                aria-selected={i === selected}
                onClick={() => onPick(i)}
              >
                <span className="switcher-preview" aria-hidden="true">
                  <span />
                  <span>
                    <i />
                    <i />
                    <i />
                    <i />
                  </span>
                </span>
                <span className="switcher-name">
                  <Icon size={15} aria-hidden="true" />
                  <span>
                    <span>{app.name}</span>
                    {detail && <span>{detail}</span>}
                  </span>
                </span>
              </div>
            )
          })}
        </div>
        <div className="switcher-hints" aria-hidden="true">
          <span>{t.switcher.hintMove}</span>
          <span>{t.switcher.hintBack}</span>
          <span>{t.switcher.hintCommit}</span>
          <span>{t.switcher.hintCancel}</span>
        </div>
      </div>
    </div>
  )
}
