// Questions the shell asks the user: a choice between a few answers, or 1
// line of text. `choose` and `askText` return a promise with the answer, so
// the calling code reads top to bottom.
import type { ReactNode } from 'react'
import { createStore, useStore } from '../lib/store'

export interface Choice<T> {
  label: string
  value: T
  kind?: 'primary' | 'danger'
}

export type Question =
  /** A dialog with content of its own, such as the properties of a file. */
  | { type: 'panel'; title: string; render: (close: () => void) => ReactNode; answer: (value: null) => void }
  | { type: 'choose'; title: string; body: string; choices: Choice<unknown>[]; answer: (value: unknown) => void }
  | {
      type: 'text'
      title: string
      label: string
      initial: string
      submit: string
      /** Returns an error text for a value that is not allowed, or null. */
      check?: (value: string) => string | null
      answer: (value: string | null) => void
    }

const store = createStore<Question | null>(null)
export const useQuestion = () => useStore(store)

/** Closes the open question with "no answer". */
export function dismissQuestion(): void {
  const open = store.get()
  if (!open) return
  store.set(null)
  open.answer(null)
}

/** Shows a question with buttons. Resolves to the chosen value, or to null
 * if the user closed the dialog. */
export function choose<T>(title: string, body: string, choices: Choice<T>[]): Promise<T | null> {
  dismissQuestion()
  return new Promise((resolve) => {
    store.set({
      type: 'choose',
      title,
      body,
      choices,
      answer: (value) => {
        store.set(null)
        resolve(value as T | null)
      },
    })
  })
}

/** Shows a dialog whose content the caller draws. `render` gets a function
 * that closes it. */
export function showPanel(title: string, render: (close: () => void) => ReactNode): void {
  dismissQuestion()
  store.set({ type: 'panel', title, render, answer: () => store.set(null) })
}

/** Asks for 1 line of text. Resolves to the trimmed text, or to null. */
export function askText(options: { title: string; label: string; initial?: string; submit: string; check?: (value: string) => string | null }): Promise<string | null> {
  dismissQuestion()
  return new Promise((resolve) => {
    store.set({
      type: 'text',
      title: options.title,
      label: options.label,
      initial: options.initial ?? '',
      submit: options.submit,
      check: options.check,
      answer: (value) => {
        store.set(null)
        resolve(value)
      },
    })
  })
}
