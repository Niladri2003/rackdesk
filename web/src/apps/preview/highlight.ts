// A small highlighter for the text preview. It marks comments, strings,
// numbers and config keys. It returns pieces of text with a class name, and
// the preview draws them as text nodes. No HTML is built from file content.
import type { Syntax } from '../files/kinds'

export interface Piece {
  text: string
  /** A CSS class, or an empty string for plain text. */
  kind: '' | 'tok-comment' | 'tok-string' | 'tok-number' | 'tok-key' | 'tok-heading'
}

// In order: a quoted string, a comment to the end of the line, a number.
const CODE = /("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')|((?:^|\s)(?:#|\/\/).*$)|(\b\d+(?:\.\d+)?\b)/g
const CONFIG_KEY = /^(\s*-?\s*)([\w.-]+)(\s*[:=])/

function scan(text: string, out: Piece[]): void {
  let last = 0
  for (const match of text.matchAll(CODE)) {
    const at = match.index
    if (at > last) out.push({ text: text.slice(last, at), kind: '' })
    out.push({ text: match[0], kind: match[1] ? 'tok-string' : match[2] ? 'tok-comment' : 'tok-number' })
    last = at + match[0].length
  }
  if (last < text.length) out.push({ text: text.slice(last), kind: '' })
}

export function highlightLine(line: string, syntax: Syntax): Piece[] {
  if (syntax === 'plain' || line.length > 2000) return [{ text: line, kind: '' }]
  if (syntax === 'markdown') {
    return [{ text: line, kind: /^#{1,6}\s/.test(line) ? 'tok-heading' : '' }]
  }
  const out: Piece[] = []
  let rest = line
  if (syntax === 'config' && !line.trimStart().startsWith('#')) {
    const key = CONFIG_KEY.exec(line)
    if (key) {
      out.push({ text: key[1], kind: '' }, { text: key[2], kind: 'tok-key' }, { text: key[3], kind: '' })
      rest = line.slice(key[0].length)
    }
  }
  scan(rest, out)
  return out
}
