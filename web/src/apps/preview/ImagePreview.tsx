// A picture with zoom. It starts scaled to fit the window. The buttons, the
// keyboard and Ctrl + wheel (or a pinch on a trackpad) change the zoom, and
// a zoomed picture is moved by dragging or by scrolling.
import { Minus, Plus } from 'lucide-react'
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { KeyboardEvent, PointerEvent } from 'react'
import { t } from '../../i18n'

const MIN_ZOOM = 0.05
const MAX_ZOOM = 8
/** The zoom buttons and keys move between these levels. */
const STEPS = [0.05, 0.1, 0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4, 6, 8]

interface Size {
  w: number
  h: number
}

interface Props {
  src: string
  name: string
  onError: () => void
}

export function ImagePreview({ src, name, onError }: Props) {
  const [area, setArea] = useState<HTMLDivElement | null>(null)
  const [natural, setNatural] = useState<Size | null>(null)
  const [room, setRoom] = useState<Size>({ w: 0, h: 0 })
  // 'fit' follows the window size. A number is a fixed scale, 1 = real size.
  const [zoom, setZoom] = useState<number | 'fit'>('fit')
  const drag = useRef<{ x: number; y: number; left: number; top: number } | null>(null)
  // The point of the picture at the centre of the view, as a share of its
  // size. It is kept in place when the zoom changes.
  const centre = useRef({ x: 0.5, y: 0.5 })

  useEffect(() => {
    if (!area) return
    const measure = () => setRoom({ w: area.clientWidth, h: area.clientHeight })
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(area)
    return () => observer.disconnect()
  }, [area])

  const fit = natural && room.w > 0 ? Math.min(1, room.w / natural.w, room.h / natural.h) : 1
  const scale = zoom === 'fit' ? fit : zoom

  const zoomTo = useCallback(
    (next: number | 'fit') => {
      if (area && area.scrollWidth > 0) {
        centre.current = {
          x: (area.scrollLeft + area.clientWidth / 2) / area.scrollWidth,
          y: (area.scrollTop + area.clientHeight / 2) / area.scrollHeight,
        }
      }
      setZoom(next === 'fit' ? 'fit' : Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, next)))
    },
    [area],
  )
  const zoomIn = () => zoomTo(STEPS.find((s) => s > scale * 1.001) ?? MAX_ZOOM)
  const zoomOut = () => zoomTo([...STEPS].reverse().find((s) => s < scale * 0.999) ?? MIN_ZOOM)

  // After a zoom change, scroll so the same point is at the centre again.
  useLayoutEffect(() => {
    if (!area) return
    area.scrollTo({
      left: centre.current.x * area.scrollWidth - area.clientWidth / 2,
      top: centre.current.y * area.scrollHeight - area.clientHeight / 2,
    })
  }, [area, scale])

  // Ctrl + wheel, which is also what a trackpad pinch sends. React's own
  // wheel handler can't stop the browser from zooming the whole page.
  useEffect(() => {
    if (!area) return
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return
      e.preventDefault()
      // A pinch sends many small steps and a mouse wheel sends a few large
      // ones. The limit makes 1 wheel step change the zoom by about a third.
      const step = Math.max(-30, Math.min(30, e.deltaY))
      zoomTo(scale * Math.exp(-step * 0.01))
    }
    area.addEventListener('wheel', onWheel, { passive: false })
    return () => area.removeEventListener('wheel', onWheel)
  }, [area, scale, zoomTo])

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return
    if (e.key === '+' || e.key === '=') zoomIn()
    else if (e.key === '-') zoomOut()
    else if (e.key === '0') zoomTo('fit')
    else if (e.key === '1') zoomTo(1)
    else return
    e.preventDefault()
  }

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || !area || (e.target as HTMLElement).closest('button')) return
    drag.current = { x: e.clientX, y: e.clientY, left: area.scrollLeft, top: area.scrollTop }
    e.currentTarget.setPointerCapture(e.pointerId)
  }
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current
    if (d && area) area.scrollTo({ left: d.left - (e.clientX - d.x), top: d.top - (e.clientY - d.y) })
  }
  const endDrag = () => {
    drag.current = null
  }

  const movable = natural !== null && (natural.w * scale > room.w + 1 || natural.h * scale > room.h + 1)
  const percent = Math.round(scale * 100)
  const z = t.preview.zoom

  return (
    <>
      <div
        ref={setArea}
        className="preview-zoom"
        tabIndex={0}
        role="group"
        aria-label={z.area(name)}
        data-movable={movable}
        onKeyDown={onKeyDown}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onDoubleClick={(e) => {
          if (!(e.target as HTMLElement).closest('button')) zoomTo(zoom === 'fit' ? 1 : 'fit')
        }}
      >
        <img
          src={src}
          alt={name}
          draggable={false}
          // Hidden until its size is known, so it is never drawn unscaled.
          style={natural ? { width: natural.w * scale, height: natural.h * scale } : { visibility: 'hidden' }}
          onLoad={(e) => setNatural({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })}
          onError={onError}
        />
      </div>
      <div className="panel preview-zoombar" role="toolbar" aria-label={z.toolbar}>
        <button type="button" className="icon-btn" aria-label={z.out} title={z.out} disabled={scale <= MIN_ZOOM} onClick={zoomOut}>
          <Minus size={15} aria-hidden="true" />
        </button>
        <button
          type="button"
          className="preview-zoom-level mono"
          aria-label={zoom === 'fit' ? z.actual(percent) : z.fit(percent)}
          title={zoom === 'fit' ? z.actualTitle : z.fitTitle}
          onClick={() => zoomTo(zoom === 'fit' ? 1 : 'fit')}
        >
          {percent}%
        </button>
        <button type="button" className="icon-btn" aria-label={z.in} title={z.in} disabled={scale >= MAX_ZOOM} onClick={zoomIn}>
          <Plus size={15} aria-hidden="true" />
        </button>
      </div>
    </>
  )
}
