// Sets up CodeMirror for the text editor. The library and the grammar of
// the file's language are loaded on demand.

export interface EditorHandle {
  /** The text as it is saved, with the line endings the file came with. */
  text: () => string
  lineCount: () => number
  destroy: () => void
}

interface Options {
  parent: HTMLElement
  text: string
  fileName: string
  /** Name for screen readers. */
  label: string
  /** Called after each change to the text, with the number of lines. */
  onChange: (lineCount: number) => void
  /** Called for Ctrl+S or Cmd+S. */
  onSave: () => void
}

export async function createEditor(options: Options): Promise<EditorHandle> {
  const [{ EditorView, basicSetup }, { EditorState }, { keymap }, { HighlightStyle, LanguageDescription, syntaxHighlighting }, { languages }, { tags }] =
    await Promise.all([
      import('codemirror'),
      import('@codemirror/state'),
      import('@codemirror/view'),
      import('@codemirror/language'),
      import('@codemirror/language-data'),
      import('@lezer/highlight'),
    ])

  // Colours come from the design tokens, so the editor follows the theme
  // and the accent without a reload.
  const theme = EditorView.theme({
    '&': { height: '100%', color: 'var(--text)', backgroundColor: 'var(--win)', fontSize: '13px' },
    '&.cm-focused': { outline: 'none' },
    '.cm-scroller': { fontFamily: 'var(--font-mono)', lineHeight: '20px' },
    '.cm-content': { caretColor: 'var(--text)' },
    '.cm-cursor': { borderLeftColor: 'var(--text)' },
    '.cm-gutters': { backgroundColor: 'var(--win)', color: 'var(--muted)', border: 'none' },
    '.cm-activeLine': { backgroundColor: 'var(--hover)' },
    '.cm-activeLineGutter': { backgroundColor: 'var(--hover)', color: 'var(--text)' },
    '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': { backgroundColor: 'var(--sel)' },
    '.cm-selectionMatch': { backgroundColor: 'var(--acc-soft)' },
    '.cm-matchingBracket': { backgroundColor: 'var(--acc-soft)', outline: '1px solid var(--acc)' },
    '.cm-panels': { backgroundColor: 'var(--side)', color: 'var(--text)', borderColor: 'var(--line)' },
    '.cm-textfield': { backgroundColor: 'var(--field)', border: '1px solid var(--line)', borderRadius: '6px' },
    '.cm-button': { backgroundImage: 'none', backgroundColor: 'var(--win)', border: '1px solid var(--line)', borderRadius: '6px' },
    '.cm-tooltip': { backgroundColor: 'var(--win)', color: 'var(--text)', border: '1px solid var(--edge)', borderRadius: '8px', overflow: 'hidden' },
    '.cm-tooltip-autocomplete ul li[aria-selected]': { backgroundColor: 'var(--sel)', color: 'var(--text)' },
    '.cm-foldPlaceholder': { backgroundColor: 'var(--field)', border: 'none', color: 'var(--muted)' },
  })
  const highlight = HighlightStyle.define([
    { tag: [tags.comment, tags.meta], color: 'var(--muted)', fontStyle: 'italic' },
    { tag: [tags.string, tags.special(tags.string), tags.regexp], color: 'var(--syn-str)' },
    { tag: [tags.number, tags.bool, tags.null, tags.atom], color: 'var(--syn-num)' },
    { tag: [tags.keyword, tags.operatorKeyword, tags.modifier], color: 'var(--acc)' },
    { tag: [tags.propertyName, tags.attributeName, tags.definition(tags.variableName)], color: 'var(--acc)' },
    { tag: [tags.typeName, tags.className, tags.tagName], color: 'var(--syn-num)' },
    { tag: tags.heading, color: 'var(--acc)', fontWeight: '600' },
    { tag: tags.link, color: 'var(--acc)', textDecoration: 'underline' },
    { tag: tags.emphasis, fontStyle: 'italic' },
    { tag: tags.strong, fontWeight: '600' },
    { tag: tags.invalid, color: 'var(--danger)' },
  ])

  // Keep the line endings of the file. Without this, a save would turn
  // every "\r\n" into "\n".
  const crlf = options.text.includes('\r\n')
  const language = LanguageDescription.matchFilename(languages, options.fileName)
  const support = language ? await language.load().catch(() => null) : null

  const view = new EditorView({
    parent: options.parent,
    state: EditorState.create({
      doc: options.text,
      extensions: [
        basicSetup,
        theme,
        syntaxHighlighting(highlight),
        EditorState.lineSeparator.of(crlf ? '\r\n' : '\n'),
        EditorView.contentAttributes.of({ 'aria-label': options.label }),
        keymap.of([
          {
            key: 'Mod-s',
            run: () => {
              options.onSave()
              return true
            },
          },
        ]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) options.onChange(update.state.doc.lines)
        }),
        ...(support ? [support] : []),
      ],
    }),
  })

  return {
    text: () => view.state.sliceDoc(),
    lineCount: () => view.state.doc.lines,
    destroy: () => view.destroy(),
  }
}
