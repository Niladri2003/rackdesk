// One window: the title bar, the 3 buttons, the resize handles and the app
// content. An app never draws its own title bar.
import { Copy, Minus, Square, X } from 'lucide-react'
import { memo, useCallback, useRef, useState } from 'react'
import type { Dispatch, PointerEvent as ReactPointerEvent, RefObject } from 'react'
import { WindowBody } from '../apps/WindowBody'
import type { AppDef } from '../apps/registry'
import { t } from '../i18n'
import { countRender } from '../lib/renderCount'
import { geometry } from './wm'
import type { Rect, Size, Snap, Win, WmAction } from './wm'

/** The pointer must move this far before a press on the title bar is a drag. */
const DRAG_THRESHOLD = 4

export type SnapTarget = Snap | 'max' | null

interface Props {
  win: Win
  app: AppDef
  focused: boolean
  /** On small screens every window fills the desktop and can't be moved. */
  compact: boolean
  desk: Size
  deskRef: RefObject<HTMLDivElement | null>
  dispatch: Dispatch<WmAction>
  onSnapPreview: (to: SnapTarget) => void
  register: (id: number, el: HTMLElement | null) => void
}

type ResizeDir = 'e' | 's' | 'se'

/** Memoised: every prop keeps its identity unless this window changed. So a
 * drag of one window draws that window again and no other. */
export const Window = memo(function Window({ win, app, focused, compact, desk, deskRef, dispatch, onSnapPreview, register }: Props) {
  countRender(`window:${app.id}`)
  const rect: Rect = compact ? { x: 0, y: 0, w: desk.w, h: desk.h } : geometry(win, desk)
  const id = win.id
  const setParams = useCallback(
    (params: Record<string, string>) => dispatch({ type: 'setParams', id, params }),
    [dispatch, id],
  )
  const [dragging, setDragging] = useState(false)
  const drag = useRef<{
    sx: number
    sy: number
    start: Rect
    floating: boolean
    moved: boolean
    snap: SnapTarget
  } | null>(null)
  const resize = useRef<{ sx: number; sy: number; start: Rect; dir: ResizeDir } | null>(null)

  const filled = win.max || win.snap !== null || compact
  const detail = app.detail?.(win.params) ?? ''
  const Icon = app.icon

  const deskOrigin = () => {
    const r = deskRef.current?.getBoundingClientRect()
    return { left: r?.left ?? 0, top: r?.top ?? 0 }
  }

  const onTitleDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (compact || e.button !== 0 || (e.target as HTMLElement).closest('button')) return
    e.currentTarget.setPointerCapture(e.pointerId)
    drag.current = {
      sx: e.clientX,
      sy: e.clientY,
      start: rect,
      floating: !win.max && win.snap === null,
      moved: false,
      snap: null,
    }
  }

  const onTitleMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current
    if (!d) return
    const dx = e.clientX - d.sx
    const dy = e.clientY - d.sy
    if (!d.moved && Math.abs(dx) + Math.abs(dy) < DRAG_THRESHOLD) return
    d.moved = true
    setDragging(true)
    const origin = deskOrigin()
    let x: number
    let y: number
    if (d.floating) {
      x = d.start.x + dx
      y = d.start.y + dy
    } else {
      // Dragging a maximised or snapped window restores its own size. The
      // pointer keeps the same relative place on the title bar.
      const rel = (d.sx - origin.left - d.start.x) / d.start.w
      x = e.clientX - origin.left - rel * win.w
      y = e.clientY - origin.top - 18
    }
    dispatch({ type: 'move', id: win.id, x, y, desk })
    const px = e.clientX - origin.left
    const py = e.clientY - origin.top
    const snap: SnapTarget = px < 10 ? 'left' : px > desk.w - 10 ? 'right' : py < 6 ? 'max' : null
    if (snap !== d.snap) {
      d.snap = snap
      onSnapPreview(snap)
    }
  }

  const onTitleUp = () => {
    const d = drag.current
    drag.current = null
    setDragging(false)
    if (!d) return
    onSnapPreview(null)
    if (d.moved && d.snap) dispatch({ type: 'snap', id: win.id, to: d.snap })
  }

  const onResizeDown = (dir: ResizeDir) => (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return
    e.preventDefault()
    e.stopPropagation()
    e.currentTarget.setPointerCapture(e.pointerId)
    resize.current = { sx: e.clientX, sy: e.clientY, start: rect, dir }
    dispatch({ type: 'focus', id: win.id })
    // A snapped or maximised window keeps its place when a resize starts.
    if (filled) dispatch({ type: 'move', id: win.id, x: rect.x, y: rect.y, desk })
    setDragging(true)
  }

  const onResizeMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const r = resize.current
    if (!r) return
    dispatch({
      type: 'resize',
      id: win.id,
      w: r.dir.includes('e') ? r.start.w + e.clientX - r.sx : r.start.w,
      h: r.dir.includes('s') ? r.start.h + e.clientY - r.sy : r.start.h,
    })
  }

  const onResizeUp = () => {
    resize.current = null
    setDragging(false)
  }

  const resizeHandle = (dir: ResizeDir) => (
    <div
      className={`win-resize win-resize-${dir}`}
      aria-hidden="true"
      onPointerDown={onResizeDown(dir)}
      onPointerMove={onResizeMove}
      onPointerUp={onResizeUp}
      onPointerCancel={onResizeUp}
    />
  )

  return (
    <section
      ref={(el) => register(win.id, el)}
      className="win"
      aria-label={t.window.title(app.name, detail)}
      tabIndex={-1}
      // A minimised window stays mounted, so the app keeps its state. `inert`
      // removes it from the tab order and from screen readers.
      inert={win.min}
      data-focused={focused}
      data-min={win.min}
      data-filled={filled}
      data-dragging={dragging}
      data-app={app.id}
      style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h, zIndex: win.z }}
      onPointerDown={() => dispatch({ type: 'focus', id: win.id })}
      onFocus={() => dispatch({ type: 'focus', id: win.id })}
    >
      <div className="win-frame">
        <div
          className="win-title"
          onPointerDown={onTitleDown}
          onPointerMove={onTitleMove}
          onPointerUp={onTitleUp}
          onPointerCancel={onTitleUp}
          onDoubleClick={(e) => {
            if (!compact && !(e.target as HTMLElement).closest('button')) dispatch({ type: 'toggleMax', id: win.id })
          }}
        >
          <Icon size={15} className="win-icon" aria-hidden="true" />
          <span className="win-app">{app.name}</span>
          {detail && <span className="win-detail">- {detail}</span>}
          <span className="win-spacer" />
          <button
            type="button"
            className="icon-btn"
            aria-label={t.window.minimise}
            title={t.window.minimise}
            onClick={() => dispatch({ type: 'minimise', id: win.id })}
          >
            <Minus size={15} aria-hidden="true" />
          </button>
          {!compact && (
            <button
              type="button"
              className="icon-btn"
              aria-label={filled ? t.window.restore : t.window.maximise}
              title={filled ? t.window.restore : t.window.maximise}
              onClick={() => dispatch({ type: 'toggleMax', id: win.id })}
            >
              {filled ? <Copy size={13} aria-hidden="true" /> : <Square size={13} aria-hidden="true" />}
            </button>
          )}
          <button
            type="button"
            className="icon-btn danger"
            aria-label={t.window.close}
            title={t.window.close}
            onClick={() => dispatch({ type: 'close', id: win.id })}
          >
            <X size={16} aria-hidden="true" />
          </button>
        </div>
        <div className="win-body">
          <WindowBody app={app} params={win.params} setParams={setParams} />
        </div>
      </div>
      {!compact && (
        <>
          {resizeHandle('e')}
          {resizeHandle('s')}
          {resizeHandle('se')}
        </>
      )}
    </section>
  )
})
