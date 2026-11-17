// Tells open file browser windows that a folder changed, so they load it
// again. A change is announced by the code that made it: an upload that
// finished, a rename, a delete. Nothing watches the server.
import { useEffect } from 'react'

type Listener = (dir: string) => void
const listeners = new Set<Listener>()

/** The path of the trash view. It is not a folder on the server. */
export const TRASH = ':trash'

/** Call after a change to the entries of `dir`. */
export function folderChanged(dir: string): void {
  listeners.forEach((listener) => listener(dir))
}

/** Runs `onChange` when `dir` is announced as changed. */
export function useFolderChanges(dir: string | undefined, onChange: () => void): void {
  useEffect(() => {
    const listener: Listener = (changed) => {
      if (changed === dir) onChange()
    }
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
    }
  }, [dir, onChange])
}
