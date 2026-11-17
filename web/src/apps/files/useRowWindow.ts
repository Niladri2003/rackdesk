// Works out which rows of a long list are on screen. Every row has the same
// height, so the answer comes from the scroll position alone and nothing is
// measured or cached. The grid view uses it with several tiles per row.
import { useCallback, useEffect, useState } from 'react'

interface Viewport {
  top: number
  height: number
  width: number
}

export interface RowWindow {
  /** Width of the scroll area in pixels. */
  width: number
  /** First and last row to draw, both included. `last` is -1 for no rows. */
  first: number
  last: number
  /** Scrolls only as far as needed to bring a row fully into view. */
  reveal: (row: number) => void
}

/** Rows drawn above and below the visible ones, so a fast scroll shows no gap. */
const OVERSCAN = 6

/** `el` is the scroll area, or null while it is not on the page. */
export function useRowWindow(el: HTMLElement | null, rowCount: number, rowHeight: number): RowWindow {
  const [view, setView] = useState<Viewport>({ top: 0, height: 0, width: 0 })

  useEffect(() => {
    if (!el) return
    const update = () =>
      setView((prev) => {
        const next = { top: el.scrollTop, height: el.clientHeight, width: el.clientWidth }
        return prev.top === next.top && prev.height === next.height && prev.width === next.width ? prev : next
      })
    update()
    el.addEventListener('scroll', update, { passive: true })
    const observer = new ResizeObserver(update)
    observer.observe(el)
    return () => {
      el.removeEventListener('scroll', update)
      observer.disconnect()
    }
  }, [el])

  const reveal = useCallback(
    (row: number) => {
      if (!el) return
      const top = row * rowHeight
      if (top < el.scrollTop) el.scrollTo({ top })
      else if (top + rowHeight > el.scrollTop + el.clientHeight) el.scrollTo({ top: top + rowHeight - el.clientHeight })
    },
    [el, rowHeight],
  )

  const first = Math.max(0, Math.floor(view.top / rowHeight) - OVERSCAN)
  const last = Math.min(rowCount - 1, Math.ceil((view.top + view.height) / rowHeight) + OVERSCAN)
  return { width: view.width, first, last, reveal }
}
