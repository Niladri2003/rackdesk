// The window manager state. It is a pure reducer: no DOM and no timers, so
// the rules are easy to read and to test.
//
// Coordinates are in CSS pixels, relative to the desktop area (the part of
// the page right of the taskbar).

export type AppId = 'files' | 'preview' | 'editor' | 'terminal' | 'settings'
export type Snap = 'left' | 'right'

export interface Size {
  w: number
  h: number
}

export interface Rect extends Size {
  x: number
  y: number
}

export interface Win extends Rect {
  id: number
  app: AppId
  /** Stacking order. A higher number is in front. */
  z: number
  min: boolean
  max: boolean
  snap: Snap | null
  /** Data for the app, for example the Settings section to show. */
  params: Record<string, string>
}

export interface WmState {
  wins: Win[]
  /** Id of the focused window, or null. */
  focus: number | null
  nextId: number
  topZ: number
}

export type WmAction =
  | { type: 'open'; app: AppId; size: Size; desk: Size; params?: Record<string, string> }
  | { type: 'close'; id: number }
  | { type: 'focus'; id: number }
  | { type: 'minimise'; id: number }
  | { type: 'toggleMax'; id: number }
  | { type: 'snap'; id: number; to: Snap | 'max' | null }
  | { type: 'move'; id: number; x: number; y: number; desk: Size }
  | { type: 'resize'; id: number; w: number; h: number }
  | { type: 'setParams'; id: number; params: Record<string, string> }
  | { type: 'restore'; state: WmState }
  | { type: 'reset' }

export const MIN_W = 480
export const MIN_H = 320
export const TITLE_H = 38
/** Windows start at z 10. The design keeps 10 to 398 for windows. */
const BASE_Z = 10

export const emptyWm: WmState = { wins: [], focus: null, nextId: 1, topZ: BASE_Z }

/** Where a window is drawn: full desk when maximised, half when snapped. */
export function geometry(win: Win, desk: Size): Rect {
  if (win.max) return { x: 0, y: 0, w: desk.w, h: desk.h }
  const half = Math.round(desk.w / 2)
  if (win.snap === 'left') return { x: 0, y: 0, w: half, h: desk.h }
  if (win.snap === 'right') return { x: half, y: 0, w: desk.w - half, h: desk.h }
  return { x: win.x, y: win.y, w: win.w, h: win.h }
}

/** The visible window that is in front, or null. */
export function topWindow(wins: Win[]): number | null {
  let top: Win | null = null
  for (const w of wins) if (!w.min && (top === null || w.z > top.z)) top = w
  return top ? top.id : null
}

/** Windows from most recently used to least. */
export function recentFirst(wins: Win[]): Win[] {
  return [...wins].sort((a, b) => b.z - a.z)
}

/** Keeps a window reachable: part of the title bar stays inside the desk. */
export function clampPosition(x: number, y: number, w: number, desk: Size): { x: number; y: number } {
  return {
    x: Math.round(Math.max(80 - w, Math.min(x, desk.w - 80))),
    y: Math.round(Math.max(0, Math.min(y, desk.h - TITLE_H))),
  }
}

function patch(state: WmState, id: number, change: (w: Win) => Partial<Win>): WmState {
  return { ...state, wins: state.wins.map((w) => (w.id === id ? { ...w, ...change(w) } : w)) }
}

export function wmReducer(state: WmState, action: WmAction): WmState {
  switch (action.type) {
    case 'open': {
      const { desk } = action
      const w = Math.max(Math.min(action.size.w, desk.w - 48), Math.min(MIN_W, desk.w))
      const h = Math.max(Math.min(action.size.h, desk.h - 72), Math.min(MIN_H, desk.h))
      // Each new window is offset from the last, in a cycle of 5.
      const step = state.wins.length % 5
      const x = Math.max(16, Math.round((desk.w - w) / 2) - 40 + step * 30)
      const y = Math.max(16, Math.round((desk.h - h) / 2) - 10 + step * 26)
      const z = state.topZ + 1
      const win: Win = {
        id: state.nextId,
        app: action.app,
        ...clampPosition(x, y, w, desk),
        w,
        h,
        z,
        min: false,
        max: false,
        snap: null,
        params: action.params ?? {},
      }
      return { wins: [...state.wins, win], focus: win.id, nextId: state.nextId + 1, topZ: z }
    }
    case 'close': {
      const wins = state.wins.filter((w) => w.id !== action.id)
      return { ...state, wins, focus: state.focus === action.id ? topWindow(wins) : state.focus }
    }
    case 'focus': {
      const win = state.wins.find((w) => w.id === action.id)
      if (!win) return state
      if (state.focus === win.id && !win.min && win.z === state.topZ) return state
      const z = state.topZ + 1
      return { ...patch(state, win.id, () => ({ z, min: false })), focus: win.id, topZ: z }
    }
    case 'minimise': {
      const next = patch(state, action.id, () => ({ min: true }))
      return { ...next, focus: topWindow(next.wins) }
    }
    case 'toggleMax':
      return patch(state, action.id, (w) => ({ max: !(w.max || w.snap), snap: null }))
    case 'snap':
      return patch(state, action.id, () =>
        action.to === 'max' ? { max: true, snap: null } : { max: false, snap: action.to },
      )
    case 'move':
      return patch(state, action.id, (w) => ({
        ...clampPosition(action.x, action.y, w.w, action.desk),
        max: false,
        snap: null,
      }))
    case 'resize':
      return patch(state, action.id, () => ({
        w: Math.round(Math.max(MIN_W, action.w)),
        h: Math.round(Math.max(MIN_H, action.h)),
        max: false,
        snap: null,
      }))
    case 'setParams':
      return patch(state, action.id, (w) => ({ params: { ...w.params, ...action.params } }))
    case 'restore':
      return action.state
    case 'reset':
      return emptyWm
  }
}

const isNumber = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v)

/** Rebuilds the state from saved data. The data comes from localStorage, so
 * every field is checked and anything unknown is dropped. */
export function parseSaved(raw: unknown, knownApps: readonly string[]): WmState {
  if (typeof raw !== 'object' || raw === null) return emptyWm
  const list = (raw as { wins?: unknown }).wins
  if (!Array.isArray(list)) return emptyWm
  const wins: Win[] = []
  for (const item of list.slice(0, 32)) {
    const w = item as Partial<Record<keyof Win, unknown>>
    if (typeof w.app !== 'string' || !knownApps.includes(w.app)) continue
    if (![w.x, w.y, w.w, w.h, w.z].every(isNumber)) continue
    const params: Record<string, string> = {}
    if (typeof w.params === 'object' && w.params !== null) {
      for (const [k, v] of Object.entries(w.params)) if (typeof v === 'string') params[k] = v.slice(0, 200)
    }
    wins.push({
      id: wins.length + 1,
      app: w.app as AppId,
      x: w.x as number,
      y: w.y as number,
      w: Math.max(MIN_W, w.w as number),
      h: Math.max(MIN_H, w.h as number),
      z: w.z as number,
      min: w.min === true,
      max: w.max === true,
      snap: w.snap === 'left' || w.snap === 'right' ? w.snap : null,
      params,
    })
  }
  // Renumber the stacking order from the base, keeping the saved order.
  const order = [...wins].sort((a, b) => a.z - b.z)
  order.forEach((w, i) => (w.z = BASE_Z + 1 + i))
  return { wins, focus: topWindow(wins), nextId: wins.length + 1, topZ: BASE_Z + wins.length }
}
