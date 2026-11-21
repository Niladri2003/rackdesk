// Draws the question that `dialog.ts` holds, as a dialog over the desktop.
import { CircleAlert } from 'lucide-react'
import { Dialog } from 'radix-ui'
import { memo, useState } from 'react'
import type { SubmitEvent } from 'react'
import { t } from '../i18n'
import { dismissQuestion, useQuestion } from './dialog'
import type { Question } from './dialog'

export const Dialogs = memo(function Dialogs() {
  const question = useQuestion()
  if (!question) return null
  return (
    <Dialog.Root open onOpenChange={(open) => !open && dismissQuestion()}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-scrim" />
        <Dialog.Content className="panel dialog" role={question.type === 'choose' ? 'alertdialog' : 'dialog'} aria-describedby={question.type === 'choose' ? 'rd-dialog-body' : undefined}>
          <Dialog.Title className="dialog-title">{question.title}</Dialog.Title>
          {question.type === 'panel' ? (
            question.render(dismissQuestion)
          ) : question.type === 'choose' ? (
            <>
              <p id="rd-dialog-body" className="muted">
                {question.body}
              </p>
              <div className="dialog-actions">
                <button type="button" className="btn" onClick={dismissQuestion}>
                  {t.common.cancel}
                </button>
                {question.choices.map((choice, i) => (
                  <button
                    key={choice.label}
                    type="button"
                    className={`btn ${choice.kind === 'primary' ? 'btn-primary' : choice.kind === 'danger' ? 'btn-danger' : ''}`}
                    autoFocus={i === question.choices.length - 1}
                    onClick={() => question.answer(choice.value)}
                  >
                    {choice.label}
                  </button>
                ))}
              </div>
            </>
          ) : (
            // A new question gets a new form, so the old text is not kept.
            <TextForm key={question.title + question.initial} question={question} />
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
})

function TextForm({ question }: { question: Extract<Question, { type: 'text' }> }) {
  const [value, setValue] = useState(question.initial)
  const [error, setError] = useState<string | null>(null)

  const submit = (e: SubmitEvent<HTMLFormElement>) => {
    e.preventDefault()
    const text = value.trim()
    const problem = question.check?.(text) ?? null
    if (problem) setError(problem)
    else question.answer(text)
  }

  return (
    <form onSubmit={submit}>
      <label className="field">
        <span>{question.label}</span>
        <input
          className="input"
          autoFocus
          spellCheck={false}
          autoComplete="off"
          aria-invalid={error !== null}
          value={value}
          onChange={(e) => {
            setValue(e.target.value)
            setError(null)
          }}
          // Select the name without its extension, as file managers do.
          onFocus={(e) => {
            const dot = e.target.value.lastIndexOf('.')
            e.target.setSelectionRange(0, dot > 0 ? dot : e.target.value.length)
          }}
        />
      </label>
      {error && (
        <div className="alert" role="alert">
          <CircleAlert size={15} aria-hidden="true" />
          <span>{error}</span>
        </div>
      )}
      <div className="dialog-actions">
        <button type="button" className="btn" onClick={dismissQuestion}>
          {t.common.cancel}
        </button>
        <button type="submit" className="btn btn-primary" disabled={!value.trim()}>
          {question.submit}
        </button>
      </div>
    </form>
  )
}
