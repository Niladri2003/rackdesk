// The file browser. It reads folders through /api/v1/fs and never changes
// anything on the server.
import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, ChevronRight, CircleAlert, ClipboardPaste, Copy, CopyPlus, Download, Eye, EyeOff, FilePlus, FolderOpen, FolderPlus, LayoutGrid, List, Pencil, Scissors, Search, Star, TextCursorInput, Trash2, Upload } from 'lucide-react'
import { ContextMenu } from 'radix-ui'
import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react'
import type { DragEvent, KeyboardEvent } from 'react'
import { api, ApiFailure, fileUrl } from '../../api/client'
import type { DirEntry } from '../../api/generated/DirEntry'
import type { Places } from '../../api/generated/Places'
import type { SortKey } from '../../api/generated/SortKey'
import { errorText, t } from '../../i18n'
import { useShell } from '../../shell/context'
import { startUploads } from '../../shell/transfers'
import type { AppProps } from '../registry'
import { duplicateEntry, newFile, newFolder, pasteInto, renameEntry, setClip, trashEntry, useClip } from './actions'
import { TRASH, useFolderChanges } from './changes'
import { useFavourites } from './favourites'
import { kindOf } from './kinds'
import { baseName, crumbs, formatSize, formatTime, joinPath, normalisePath, parentPath } from './paths'
import { Sidebar } from './Sidebar'
import { TrashView } from './TrashView'
import { useListing } from './useListing'
import { useRowWindow } from './useRowWindow'

const ROW_HEIGHT = 32
const TILE_WIDTH = 112
const TILE_HEIGHT = 104
/** Space around the tiles of the grid view. */
const GRID_PAD = 8
const SORTS: SortKey[] = ['name', 'size', 'modified']
/** A text file up to this size opens in the editor. A larger one opens in
 * the preview, which reads only its start. */
const MAX_EDIT_BYTES = 2 * 1024 * 1024
const f = t.files

/** Starts a download of a file through the browser. */
function download(path: string) {
  const link = document.createElement('a')
  link.href = fileUrl(path)
  link.download = baseName(path)
  link.click()
}

export function Files({ params, setParams }: AppProps) {
  const shell = useShell()
  const { sessionEnded } = shell
  const id = useId()
  const [places, setPlaces] = useState<Places | null>(null)
  const favourites = useFavourites(shell.session.username)
  const clip = useClip()
  const fileInput = useRef<HTMLInputElement>(null)
  const [dropping, setDropping] = useState(false)

  // What the window shows. These survive a reload, through `params`.
  const path = params.path
  const view = params.view === 'grid' ? 'grid' : 'list'
  const sort: SortKey = SORTS.includes(params.sort as SortKey) ? (params.sort as SortKey) : 'name'
  const desc = params.desc === '1'
  const hidden = params.hidden === '1'

  const [filterText, setFilterText] = useState('')
  const [filter, setFilter] = useState('')
  const [selected, setSelected] = useState(-1)
  const [history, setHistory] = useState<{ stack: string[]; at: number }>({ stack: [], at: -1 })
  const [editingPath, setEditingPath] = useState(false)

  // Places: loaded once when the window opens. Also the start folder.
  useEffect(() => {
    let live = true
    api.fs.places().then(
      (value) => {
        if (!live) return
        setPlaces(value)
        if (!path) setParams({ path: value.home ?? '/' })
      },
      (e: unknown) => {
        if (!live) return
        if (e instanceof ApiFailure && e.code === 'unauthorized') sessionEnded()
        else if (!path) setParams({ path: '/' })
      },
    )
    return () => {
      live = false
    }
    // Runs once per window. `path` is read only to choose the start folder.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  // The filter is sent 200 ms after the last key press.
  useEffect(() => {
    const timer = setTimeout(() => setFilter(filterText.trim()), 200)
    return () => clearTimeout(timer)
  }, [filterText])

  const query = useMemo(() => ({ path: path ?? '/', sort, desc, hidden, filter }), [path, sort, desc, hidden, filter])
  const listing = useListing(query)
  const { error } = listing
  useEffect(() => {
    if (error instanceof ApiFailure && error.code === 'unauthorized') sessionEnded()
  }, [error, sessionEnded])
  // Load the folder again after an upload, a rename or a delete in it.
  useFolderChanges(path, listing.reload)

  const go = useCallback(
    (target: string, mode: 'push' | 'history' = 'push') => {
      // The trash is a special view, not a path on the server.
      const next = target === TRASH ? TRASH : normalisePath(target)
      setFilterText('')
      setFilter('')
      setSelected(-1)
      setEditingPath(false)
      if (mode === 'push' && path && next !== path) {
        setHistory((h) => {
          const stack = [...h.stack.slice(0, h.at + 1), path].slice(-50)
          return { stack, at: stack.length - 1 }
        })
      }
      setParams({ path: next })
    },
    [path, setParams],
  )

  const canBack = history.at >= 0
  const canForward = history.at < history.stack.length - 1
  const back = () => {
    if (!canBack || !path) return
    const target = history.stack[history.at]
    setHistory((h) => {
      const stack = [...h.stack]
      stack[h.at] = path
      return { stack, at: h.at - 1 }
    })
    go(target, 'history')
  }
  const forward = () => {
    if (!canForward || !path) return
    const target = history.stack[history.at + 1]
    setHistory((h) => {
      const stack = [...h.stack]
      stack[h.at + 1] = path
      return { stack, at: h.at + 1 }
    })
    go(target, 'history')
  }

  const open = useCallback(
    (entry: DirEntry) => {
      if (!path) return
      const full = joinPath(path, entry.name)
      const kind = kindOf(entry)
      if (kind.open === 'folder') go(full)
      else if (kind.open === 'text' && entry.size <= MAX_EDIT_BYTES) shell.openApp('editor', { path: full })
      else if (kind.open !== 'none') shell.openApp('preview', { path: full })
      else if (entry.kind === 'file' || entry.target === 'file') download(full)
    },
    [path, go, shell],
  )

  // ---------- The list ----------
  // Every row has the same height. In the grid view a row holds several
  // tiles, so an entry's place is: row = index / columns, column = the rest.
  const [scrollEl, setScrollEl] = useState<HTMLDivElement | null>(null)
  const rowHeight = view === 'grid' ? TILE_HEIGHT : ROW_HEIGHT
  const probe = useRowWindow(scrollEl, 0, rowHeight)
  const columns = view === 'grid' ? Math.max(1, Math.floor((probe.width - 2 * GRID_PAD) / TILE_WIDTH)) : 1
  const rowCount = Math.ceil(listing.total / columns)
  const rows = useRowWindow(scrollEl, rowCount, rowHeight)
  const first = rows.first * columns
  const last = Math.min(listing.total - 1, (rows.last + 1) * columns - 1)
  const indexes: number[] = []
  for (let i = first; i <= last; i++) indexes.push(i)
  const { need } = listing
  useEffect(() => {
    if (last >= first) need(first, last)
  }, [first, last, need])

  // Back at the top when the folder or the order changes.
  useEffect(() => {
    scrollEl?.scrollTo({ top: 0 })
  }, [query, scrollEl])

  const select = (index: number) => {
    const clamped = Math.max(0, Math.min(listing.total - 1, index))
    setSelected(clamped)
    rows.reveal(Math.floor(clamped / columns))
  }

  const onListKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!path || e.altKey) return
    const entry = listing.entry(selected)
    if (e.ctrlKey || e.metaKey) {
      const key = e.key.toLowerCase()
      if (key === 'v') void pasteInto(path)
      else if (!entry) return
      else if (key === 'c') setClip('copy', joinPath(path, entry.name))
      else if (key === 'x') setClip('cut', joinPath(path, entry.name))
      else if (key === 'd') void duplicateEntry(path, entry.name)
      else return
      e.preventDefault()
      return
    }
    if (entry && (e.key === 'F2' || e.key === 'Delete')) {
      e.preventDefault()
      if (e.key === 'F2') void renameEntry(path, entry.name)
      else void trashEntry(path, entry.name)
      return
    }
    const step: Record<string, number> = { ArrowDown: columns, ArrowUp: -columns, ArrowRight: view === 'grid' ? 1 : 0, ArrowLeft: view === 'grid' ? -1 : 0 }
    if (step[e.key]) {
      e.preventDefault()
      select(selected < 0 ? 0 : selected + step[e.key])
    } else if (e.key === 'Home') {
      e.preventDefault()
      select(0)
    } else if (e.key === 'End') {
      e.preventDefault()
      select(listing.total - 1)
    } else if (e.key === 'Enter') {
      if (entry) {
        e.preventDefault()
        open(entry)
      }
    } else if (e.key === 'Backspace' && path && path !== '/') {
      e.preventDefault()
      go(parentPath(path))
    }
  }

  if (!path) {
    return (
      <div className="empty-state" role="status">
        <span className="spinner" aria-hidden="true" />
        <p className="muted">{f.loading}</p>
      </div>
    )
  }

  // The scroll area is a list box only while it holds rows. An empty or
  // failed folder shows a message, which is not a list.
  const hasRows = !listing.error && !listing.loading && listing.total > 0
  const inTrash = path === TRASH
  const openInEditor = (file: string) => shell.openApp('editor', { path: file })
  const selectedEntry = selected >= 0 && !inTrash ? listing.entry(selected) : undefined
  const selectedKind = selectedEntry ? kindOf(selectedEntry) : undefined
  const isFavourite = favourites.list.includes(path)
  const selectedPath = selectedEntry ? joinPath(path, selectedEntry.name) : ''
  const selectedIsFile = selectedEntry ? selectedEntry.kind === 'file' || selectedEntry.target === 'file' : false

  // Files dragged in from the user's computer are uploaded to this folder.
  const hasFiles = (e: DragEvent) => e.dataTransfer.types.includes('Files')
  const onDragOver = (e: DragEvent) => {
    if (inTrash || !hasFiles(e)) return
    e.preventDefault()
    setDropping(true)
  }
  const onDrop = (e: DragEvent) => {
    if (inTrash || !hasFiles(e)) return
    e.preventDefault()
    setDropping(false)
    // A dropped folder has no type and no size. Folder upload is not built.
    startUploads([...e.dataTransfer.files].filter((file) => file.type !== '' || file.size > 0), path)
  }
  const setSort = (key: SortKey) => setParams(key === sort ? { desc: desc ? '0' : '1' } : { sort: key, desc: '0' })
  const optionId = (index: number) => `${id}-row-${index}`
  const sortButton = (key: SortKey, label: string) => (
    <button
      type="button"
      className={`files-col files-col-${key}`}
      aria-label={f.sortBy(label)}
      aria-pressed={sort === key}
      onClick={() => setSort(key)}
    >
      {label}
      {sort === key && (desc ? <ArrowDown size={12} aria-hidden="true" /> : <ArrowUp size={12} aria-hidden="true" />)}
    </button>
  )

  return (
    <div className="files">
      <div className="files-bar">
        <button type="button" className="icon-btn" aria-label={f.back} title={f.back} disabled={!canBack} onClick={back}>
          <ArrowLeft size={16} aria-hidden="true" />
        </button>
        <button type="button" className="icon-btn" aria-label={f.forward} title={f.forward} disabled={!canForward} onClick={forward}>
          <ArrowRight size={16} aria-hidden="true" />
        </button>
        <button type="button" className="icon-btn" aria-label={f.up} title={f.up} disabled={path === '/' || inTrash} onClick={() => go(parentPath(path))}>
          <ArrowUp size={16} aria-hidden="true" />
        </button>

        {editingPath ? (
          <form
            className="files-path"
            onSubmit={(e) => {
              e.preventDefault()
              go(new FormData(e.currentTarget).get('path') as string)
            }}
          >
            <input
              name="path"
              className="mono"
              aria-label={f.pathInput}
              defaultValue={path}
              autoFocus
              spellCheck={false}
              autoComplete="off"
              onBlur={() => setEditingPath(false)}
              onKeyDown={(e) => e.key === 'Escape' && setEditingPath(false)}
            />
          </form>
        ) : (
          <nav className="files-path" aria-label={f.path}>
            <ol>
              <li>
                <button type="button" className="crumb mono" aria-current={path === '/' ? 'page' : undefined} onClick={() => go('/')}>
                  /
                </button>
              </li>
              {inTrash && (
                <li>
                  <ChevronRight size={13} aria-hidden="true" />
                  <span className="crumb" aria-current="page">
                    {f.trash}
                  </span>
                </li>
              )}
              {(inTrash ? [] : crumbs(path)).map((c) => (
                <li key={c.path}>
                  <ChevronRight size={13} aria-hidden="true" />
                  <button type="button" className="crumb mono" aria-current={c.path === path ? 'page' : undefined} onClick={() => go(c.path)}>
                    {c.name}
                  </button>
                </li>
              ))}
            </ol>
            <button type="button" className="icon-btn" aria-label={f.pathEdit} title={f.pathEdit} onClick={() => setEditingPath(true)}>
              <Pencil size={13} aria-hidden="true" />
            </button>
          </nav>
        )}

        <button type="button" className="icon-btn" aria-label={f.newFolder} title={f.newFolder} disabled={inTrash} onClick={() => void newFolder(path)}>
          <FolderPlus size={16} aria-hidden="true" />
        </button>
        <button type="button" className="icon-btn" aria-label={f.newFile} title={f.newFile} disabled={inTrash} onClick={() => void newFile(path, openInEditor)}>
          <FilePlus size={16} aria-hidden="true" />
        </button>
        <button type="button" className="icon-btn" aria-label={f.upload} title={f.upload} disabled={inTrash} onClick={() => fileInput.current?.click()}>
          <Upload size={16} aria-hidden="true" />
        </button>
        <input
          ref={fileInput}
          type="file"
          multiple
          hidden
          aria-label={f.upload}
          onChange={(e) => {
            startUploads([...(e.target.files ?? [])], path)
            // Clear it, so the same file can be chosen again.
            e.target.value = ''
          }}
        />
        <label className="files-filter">
          <Search size={14} aria-hidden="true" />
          <input
            type="search"
            aria-label={f.filter}
            placeholder={f.filter}
            spellCheck={false}
            value={filterText}
            onChange={(e) => {
              setFilterText(e.target.value)
              setSelected(-1)
            }}
          />
        </label>
        <button
          type="button"
          className="icon-btn"
          aria-label={isFavourite ? f.removeFavourite : f.addFavourite}
          title={isFavourite ? f.removeFavourite : f.addFavourite}
          aria-pressed={isFavourite}
          onClick={() => favourites.toggle(path)}
        >
          <Star size={15} aria-hidden="true" fill={isFavourite ? 'currentColor' : 'none'} />
        </button>
        <button
          type="button"
          className="icon-btn"
          aria-label={f.showHidden}
          title={f.showHidden}
          aria-pressed={hidden}
          onClick={() => setParams({ hidden: hidden ? '0' : '1' })}
        >
          {hidden ? <Eye size={15} aria-hidden="true" /> : <EyeOff size={15} aria-hidden="true" />}
        </button>
        <div className="segmented" role="radiogroup" aria-label={f.view}>
          <button type="button" role="radio" aria-checked={view === 'list'} aria-label={f.viewList} title={f.viewList} onClick={() => setParams({ view: 'list' })}>
            <List size={14} aria-hidden="true" />
          </button>
          <button type="button" role="radio" aria-checked={view === 'grid'} aria-label={f.viewGrid} title={f.viewGrid} onClick={() => setParams({ view: 'grid' })}>
            <LayoutGrid size={14} aria-hidden="true" />
          </button>
        </div>
      </div>

      <div className="files-body">
        <Sidebar path={path} places={places} favourites={favourites.list} onGo={go} />
        <div className="files-main" onDragOver={onDragOver} onDragLeave={() => setDropping(false)} onDrop={onDrop}>
          {dropping && (
            <div className="files-drop" aria-hidden="true">
              <Upload size={20} />
              {f.dropHere(baseName(path))}
            </div>
          )}
          {inTrash ? (
            <TrashView />
          ) : (
            <>
          <div className="files-head" data-view={view}>
            {sortButton('name', f.colName)}
            {sortButton('size', f.colSize)}
            {sortButton('modified', f.colModified)}
          </div>
          <ContextMenu.Root modal={false}>
          <ContextMenu.Trigger asChild>
          <div
            ref={setScrollEl}
            className="files-scroll"
            role={hasRows ? 'listbox' : undefined}
            tabIndex={hasRows ? 0 : undefined}
            aria-label={hasRows ? f.listLabel(baseName(path)) : undefined}
            aria-busy={listing.loading}
            aria-activedescendant={hasRows && selectedEntry ? optionId(selected) : undefined}
            data-view={view}
            onKeyDown={onListKey}
            onContextMenu={(e) => {
              // A right click on empty space is about the folder.
              if (!(e.target as HTMLElement).closest('.files-row')) setSelected(-1)
            }}
          >
            {listing.error ? (
              <div className="empty-state" role="alert">
                <CircleAlert size={24} aria-hidden="true" />
                <b>{errorText(listing.error)}</b>
                <div className="row">
                  {path !== '/' && (
                    <button type="button" className="btn" onClick={() => go(parentPath(path))}>
                      {f.goUp}
                    </button>
                  )}
                  <button type="button" className="btn" onClick={listing.reload}>
                    {t.common.retry}
                  </button>
                </div>
              </div>
            ) : listing.loading ? (
              <div className="empty-state" role="status">
                <span className="spinner" aria-hidden="true" />
                <p className="muted">{f.loading}</p>
              </div>
            ) : listing.total === 0 ? (
              <div className="empty-state">
                <FolderOpen size={24} aria-hidden="true" />
                <p className="muted">{filter ? f.emptyFilter : f.empty}</p>
              </div>
            ) : (
              <div className="files-canvas" style={{ height: rowCount * rowHeight + (view === 'grid' ? 2 * GRID_PAD : 0) }}>
                {indexes.map((index) => {
                  const entry = listing.entry(index)
                  const top = Math.floor(index / columns) * rowHeight
                  const style =
                    view === 'grid'
                      ? { top: top + GRID_PAD, left: GRID_PAD + (index % columns) * TILE_WIDTH, width: TILE_WIDTH, height: TILE_HEIGHT }
                      : { top, height: ROW_HEIGHT }
                  if (!entry) {
                    return (
                      <div key={index} className="files-row files-row-loading" style={style} aria-hidden="true">
                        <span className="skeleton" />
                      </div>
                    )
                  }
                  const kind = kindOf(entry)
                  const Icon = kind.icon
                  const isLink = entry.kind === 'symlink'
                  return (
                    <div
                      key={index}
                      id={optionId(index)}
                      className="files-row"
                      role="option"
                      aria-selected={index === selected}
                      aria-setsize={listing.total}
                      aria-posinset={index + 1}
                      style={style}
                      onClick={() => setSelected(index)}
                      onContextMenu={() => setSelected(index)}
                      onDoubleClick={() => open(entry)}
                    >
                      <Icon size={view === 'grid' ? 36 : 16} style={{ color: kind.tint }} aria-hidden="true" />
                      <span className="files-name" title={entry.name}>
                        {entry.name}
                      </span>
                      {isLink && (
                        <span className="badge" title={entry.target ? undefined : f.brokenLink}>
                          {f.linkTo}
                        </span>
                      )}
                      {view === 'list' && (
                        <>
                          <span className="files-size mono">{kind.open === 'folder' ? '' : formatSize(entry.size)}</span>
                          <span className="files-time">{formatTime(entry.modified)}</span>
                        </>
                      )}
                    </div>
                  )
                })}
              </div>
            )}
          </div>
          </ContextMenu.Trigger>
          <ContextMenu.Portal>
            <ContextMenu.Content className="panel menu" aria-label={f.actions}>
              {selectedEntry && selectedKind && (
                <>
                  {(selectedKind.open !== 'none' || selectedEntry.kind === 'dir') && (
                    <ContextMenu.Item className="menu-item" onSelect={() => open(selectedEntry)}>
                      <FolderOpen size={15} aria-hidden="true" />
                      {f.open}
                    </ContextMenu.Item>
                  )}
                  {selectedIsFile && (
                    <ContextMenu.Item className="menu-item" onSelect={() => download(selectedPath)}>
                      <Download size={15} aria-hidden="true" />
                      {f.download}
                    </ContextMenu.Item>
                  )}
                  <ContextMenu.Separator className="menu-separator" />
                  <ContextMenu.Item className="menu-item" onSelect={() => setClip('cut', selectedPath)}>
                    <Scissors size={15} aria-hidden="true" />
                    {f.cut}
                    <span className="menu-kbd">Ctrl+X</span>
                  </ContextMenu.Item>
                  <ContextMenu.Item className="menu-item" onSelect={() => setClip('copy', selectedPath)}>
                    <Copy size={15} aria-hidden="true" />
                    {f.copy}
                    <span className="menu-kbd">Ctrl+C</span>
                  </ContextMenu.Item>
                  <ContextMenu.Item className="menu-item" onSelect={() => void duplicateEntry(path, selectedEntry.name)}>
                    <CopyPlus size={15} aria-hidden="true" />
                    {f.duplicate}
                    <span className="menu-kbd">Ctrl+D</span>
                  </ContextMenu.Item>
                  <ContextMenu.Item className="menu-item" onSelect={() => void renameEntry(path, selectedEntry.name)}>
                    <TextCursorInput size={15} aria-hidden="true" />
                    {f.rename}
                    <span className="menu-kbd">F2</span>
                  </ContextMenu.Item>
                  <ContextMenu.Item className="menu-item danger" onSelect={() => void trashEntry(path, selectedEntry.name)}>
                    <Trash2 size={15} aria-hidden="true" />
                    {f.moveToTrash}
                    <span className="menu-kbd">Del</span>
                  </ContextMenu.Item>
                  <ContextMenu.Separator className="menu-separator" />
                </>
              )}
              <ContextMenu.Item className="menu-item" disabled={!clip} onSelect={() => void pasteInto(path)}>
                <ClipboardPaste size={15} aria-hidden="true" />
                {f.paste}
                <span className="menu-kbd">Ctrl+V</span>
              </ContextMenu.Item>
              <ContextMenu.Item className="menu-item" onSelect={() => void newFolder(path)}>
                <FolderPlus size={15} aria-hidden="true" />
                {f.newFolder}
              </ContextMenu.Item>
              <ContextMenu.Item className="menu-item" onSelect={() => void newFile(path, openInEditor)}>
                <FilePlus size={15} aria-hidden="true" />
                {f.newFile}
              </ContextMenu.Item>
              <ContextMenu.Item className="menu-item" onSelect={() => fileInput.current?.click()}>
                <Upload size={15} aria-hidden="true" />
                {f.upload}
              </ContextMenu.Item>
            </ContextMenu.Content>
          </ContextMenu.Portal>
          </ContextMenu.Root>
          <div className="files-foot">
            {selectedEntry && selectedKind ? (
              <>
                <span className="files-foot-name mono" title={selectedEntry.name}>
                  {selectedEntry.name}
                </span>
                <span>{selectedKind.label}</span>
                {selectedKind.open !== 'folder' && <span className="mono">{formatSize(selectedEntry.size)}</span>}
                <span className="win-spacer" />
                {(selectedKind.open !== 'none' || selectedEntry.kind === 'dir') && (
                  <button type="button" className="btn btn-small" onClick={() => open(selectedEntry)}>
                    {f.open}
                  </button>
                )}
                {selectedIsFile && (
                  <button type="button" className="btn btn-small" onClick={() => download(selectedPath)}>
                    <Download size={14} aria-hidden="true" />
                    {f.download}
                  </button>
                )}
                <button type="button" className="btn btn-small" onClick={() => void renameEntry(path, selectedEntry.name)}>
                  {f.rename}
                </button>
                <button
                  type="button"
                  className="icon-btn danger"
                  aria-label={f.moveToTrash}
                  title={f.moveToTrash}
                  onClick={() => void trashEntry(path, selectedEntry.name)}
                >
                  <Trash2 size={14} aria-hidden="true" />
                </button>
              </>
            ) : (
              <span role="status">{listing.loading || listing.error ? '' : filter ? f.itemsMatching(listing.total) : f.items(listing.total)}</span>
            )}
          </div>
            </>
          )}
        </div>
      </div>
    </div>
  )
}
