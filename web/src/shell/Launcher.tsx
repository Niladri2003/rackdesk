// The search dialog (Ctrl+K). One input searches apps, settings and actions.
import { ArrowDown, ArrowUp, CornerDownLeft, Search, SearchX } from 'lucide-react'
import { Dialog } from 'radix-ui'
import { useEffect, useMemo, useRef, useState } from 'react'
import type { KeyboardEvent } from 'react'
import { t } from '../i18n'
import { useShell } from './context'
import { MOD_KEY } from './icons'
import { REMOTE_MIN_CHARS, search, searchRemote } from './search'
import type { SearchGroup } from './search'

/** A remote search starts this long after the last key press. */
const REMOTE_DELAY_MS = 250

interface Props {
  onClose: () => void
  /** Called when the dialog has closed and focus can move. */
  onCloseAutoFocus: (e: Event) => void
}

export function Launcher({ onClose, onCloseAutoFocus }: Props) {
  const shell = useShell()
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState(0)
  const listRef = useRef<HTMLDivElement>(null)

  // Results from the server (files), for the query they belong to.
  const [remote, setRemote] = useState<{ query: string; groups: SearchGroup[] }>({ query: '', groups: [] })
  const wanted = query.trim().toLowerCase()
  const remoteWanted = wanted.length >= REMOTE_MIN_CHARS
  const searching = remoteWanted && remote.query !== wanted
  useEffect(() => {
    if (!remoteWanted) return
    const abort = new AbortController()
    const timer = setTimeout(() => {
      void searchRemote(wanted, abort.signal).then((groups) => {
        if (!abort.signal.aborted) setRemote({ query: wanted, groups })
      })
    }, REMOTE_DELAY_MS)
    return () => {
      clearTimeout(timer)
      abort.abort()
    }
  }, [wanted, remoteWanted])

  // Each group knows the position of its first item in the flat list.
  const groups = useMemo(() => {
    const local = search(query)
    // Files come after apps, and before settings and actions.
    const files = remoteWanted && remote.query === wanted ? remote.groups : []
    const split = local.length && local[0].label === t.search.groupApps ? 1 : 0
    const found = [...local.slice(0, split), ...files, ...local.slice(split)]
    return found.map((g, i) => ({
      ...g,
      start: found.slice(0, i).reduce((count, earlier) => count + earlier.items.length, 0),
    }))
  }, [query, remote, remoteWanted, wanted])
  const flat = groups.flatMap((g) => g.items)
  const active = flat.length ? Math.min(selected, flat.length - 1) : -1
  const optionId = (index: number) => `rd-search-option-${index}`

  const run = (index: number) => {
    const item = flat[index]
    if (!item) return
    onClose()
    item.run(shell)
  }

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      if (!flat.length) return
      const next = (active + (e.key === 'ArrowDown' ? 1 : flat.length - 1)) % flat.length
      setSelected(next)
      listRef.current?.querySelector(`#${optionId(next)}`)?.scrollIntoView({ block: 'nearest' })
    } else if (e.key === 'Enter') {
      e.preventDefault()
      run(active)
    }
  }

  return (
    <Dialog.Root open onOpenChange={(open) => !open && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="launcher-scrim" />
        <Dialog.Content className="panel launcher" aria-describedby={undefined} onCloseAutoFocus={onCloseAutoFocus}>
          <Dialog.Title className="sr-only">{t.search.label}</Dialog.Title>
          <div className="launcher-head">
            <Search size={17} className="muted" aria-hidden="true" />
            <input
              role="combobox"
              aria-label={t.search.inputLabel}
              aria-expanded="true"
              aria-controls="rd-search-list"
              aria-activedescendant={active >= 0 ? optionId(active) : undefined}
              aria-autocomplete="list"
              placeholder={t.search.placeholder}
              spellCheck={false}
              autoComplete="off"
              value={query}
              onChange={(e) => {
                setQuery(e.target.value)
                setSelected(0)
              }}
              onKeyDown={onKeyDown}
            />
            {searching && <span className="spinner" role="status" aria-label={t.search.searching} />}
            <span className="kbd" aria-hidden="true">
              esc
            </span>
          </div>
          <div id="rd-search-list" ref={listRef} className="launcher-list" role="listbox" aria-label={t.search.results}>
            {groups.map((g) => (
              <div key={g.label} role="group" aria-label={g.label}>
                <div className="launcher-group" aria-hidden="true">
                  {g.label}
                </div>
                {g.items.map((item, offset) => {
                  const i = g.start + offset
                  const Icon = item.icon
                  return (
                    <div
                      key={item.key}
                      id={optionId(i)}
                      className="launcher-item"
                      role="option"
                      aria-selected={i === active}
                      onClick={() => run(i)}
                      onPointerMove={() => i !== selected && setSelected(i)}
                    >
                      <span className="launcher-icon">
                        <Icon size={16} aria-hidden="true" />
                      </span>
                      <span className="launcher-text">
                        <span>{item.title}</span>
                        <span>{item.detail}</span>
                      </span>
                      {item.later && <span className="badge">{t.search.later}</span>}
                      {i === active && (
                        <span className="kbd" aria-hidden="true">
                          <CornerDownLeft size={11} />
                        </span>
                      )}
                    </div>
                  )
                })}
              </div>
            ))}
          </div>
          {flat.length === 0 && !searching && (
            <div className="empty-state" role="status">
              <SearchX size={24} aria-hidden="true" />
              <b>{t.search.noneTitle}</b>
              <p className="muted">{t.search.noneBody(query.trim())}</p>
            </div>
          )}
          <div className="launcher-foot" aria-hidden="true">
            <span>
              <span className="kbd">
                <ArrowUp size={10} />
              </span>
              <span className="kbd">
                <ArrowDown size={10} />
              </span>
              {t.search.move}
            </span>
            <span>
              <span className="kbd">
                <CornerDownLeft size={10} />
              </span>
              {t.search.open}
            </span>
            <span>
              <span className="kbd">esc</span>
              {t.search.close}
            </span>
            <span className="win-spacer" />
            <span>
              {t.search.openHint} <span className="kbd">{MOD_KEY}K</span>
            </span>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}
