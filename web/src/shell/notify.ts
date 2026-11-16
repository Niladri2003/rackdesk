// Notifications. `notify` shows a toast for a few seconds and adds the
// message to the list behind the bell.
import { createStore, useStore } from '../lib/store'

export type NoticeKind = 'info' | 'success' | 'warning' | 'error'

export interface Notice {
  id: number
  kind: NoticeKind
  text: string
  /** Milliseconds since the Unix epoch. */
  time: number
}

interface State {
  list: Notice[]
  toasts: Notice[]
  unread: number
}

const TOAST_MS = 5000
const MAX_KEPT = 50
const store = createStore<State>({ list: [], toasts: [], unread: 0 })
let nextId = 1

export const useNotices = () => useStore(store)

export function notify(kind: NoticeKind, text: string): void {
  const notice: Notice = { id: nextId++, kind, text, time: Date.now() }
  store.set((s) => ({
    list: [notice, ...s.list].slice(0, MAX_KEPT),
    toasts: [...s.toasts, notice],
    unread: s.unread + 1,
  }))
  setTimeout(() => dismissToast(notice.id), TOAST_MS)
}

export function dismissToast(id: number): void {
  store.set((s) => (s.toasts.some((n) => n.id === id) ? { ...s, toasts: s.toasts.filter((n) => n.id !== id) } : s))
}

export function markAllRead(): void {
  store.set((s) => (s.unread === 0 ? s : { ...s, unread: 0 }))
}

export function clearNotices(): void {
  store.set({ list: [], toasts: [], unread: 0 })
}
