// Tracks whether the server answers. When a request gets no answer, the
// shell shows a banner and this module retries with a growing delay. Nothing
// here runs while the connection is fine.
import { serverIsUp, setNetworkFailureHandler } from '../api/client'
import { createStore, useStore } from '../lib/store'

export interface Connection {
  online: boolean
  /** Seconds until the next automatic retry. 0 while a retry runs. */
  retryIn: number
}

const DELAYS = [2, 4, 8, 15, 30]
const store = createStore<Connection>({ online: true, retryIn: 0 })
let attempt = 0
let timer: ReturnType<typeof setInterval> | undefined
let onRestored: () => void = () => {}

export const useConnection = () => useStore(store)

/** Called once by the shell. `restored` runs when the server answers again. */
export function watchConnection(restored: () => void): () => void {
  onRestored = restored
  setNetworkFailureHandler(markOffline)
  window.addEventListener('offline', markOffline)
  return () => {
    setNetworkFailureHandler(() => {})
    window.removeEventListener('offline', markOffline)
    clearInterval(timer)
    timer = undefined
    store.set({ online: true, retryIn: 0 })
  }
}

function markOffline(): void {
  if (!store.get().online) return
  attempt = 0
  store.set({ online: false, retryIn: DELAYS[0] })
  clearInterval(timer)
  timer = setInterval(tick, 1000)
}

function tick(): void {
  const { online, retryIn } = store.get()
  if (online) return
  if (retryIn > 1) store.set({ online: false, retryIn: retryIn - 1 })
  else if (retryIn === 1) void retryNow()
}

export async function retryNow(): Promise<void> {
  if (store.get().online) return
  store.set({ online: false, retryIn: 0 })
  if (await serverIsUp()) {
    clearInterval(timer)
    timer = undefined
    store.set({ online: true, retryIn: 0 })
    onRestored()
    return
  }
  attempt = Math.min(attempt + 1, DELAYS.length - 1)
  store.set({ online: false, retryIn: DELAYS[attempt] })
}
