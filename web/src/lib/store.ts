// A very small state store for values shared across the shell. It works with
// React's `useSyncExternalStore`.
import { useSyncExternalStore } from 'react'

export interface Store<T> {
  get: () => T
  set: (next: T | ((prev: T) => T)) => void
  subscribe: (listener: () => void) => () => void
}

export function createStore<T>(initial: T): Store<T> {
  let value = initial
  const listeners = new Set<() => void>()
  return {
    get: () => value,
    set: (next) => {
      const resolved = typeof next === 'function' ? (next as (prev: T) => T)(value) : next
      if (Object.is(resolved, value)) return
      value = resolved
      listeners.forEach((l) => l())
    },
    subscribe: (listener) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
  }
}

export function useStore<T>(store: Store<T>): T {
  return useSyncExternalStore(store.subscribe, store.get)
}

/** Reads JSON from localStorage. Returns `null` if it is missing or broken. */
export function loadJson(key: string): unknown {
  try {
    const raw = localStorage.getItem(key)
    return raw === null ? null : JSON.parse(raw)
  } catch {
    return null
  }
}

/** Writes JSON to localStorage. A full or blocked storage is ignored. */
export function saveJson(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    // Private windows and full storage throw. The UI works without it.
  }
}
