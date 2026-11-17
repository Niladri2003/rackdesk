// What a file is, decided from its name: the label, the icon, and how the UI
// opens it. The server decides on its own which types a browser may show in
// the page, so a wrong guess here can't make a file run as a page.
import { File, FileArchive, FileCode, FileCog, FileImage, FileMusic, FileTerminal, FileText, FileVideo, Folder, Link2 } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { DirEntry } from '../../api/generated/DirEntry'
import { t } from '../../i18n'

export type OpenAs = 'folder' | 'image' | 'video' | 'audio' | 'pdf' | 'text' | 'none'

export interface KindInfo {
  label: string
  icon: LucideIcon
  /** A CSS colour for the icon. */
  tint: string
  open: OpenAs
  /** For text: which highlighting rules to use. */
  syntax: Syntax
}

export type Syntax = 'plain' | 'code' | 'config' | 'shell' | 'markdown'

const k = t.files.kinds
const tint = (hue: number) => `oklch(0.62 0.13 ${hue})`

const kind = (label: string, icon: LucideIcon, color: string, open: OpenAs, syntax: Syntax = 'plain'): KindInfo => ({
  label,
  icon,
  tint: color,
  open,
  syntax,
})

const FOLDER = kind(k.folder, Folder, 'var(--acc)', 'folder')
const PLAIN_FILE = kind(k.file, File, 'var(--muted)', 'none')
const SPECIAL = kind(k.other, File, 'var(--muted)', 'none')
const BROKEN_LINK = kind(k.link, Link2, 'var(--muted)', 'none')

const BY_EXTENSION: Record<string, KindInfo> = {}
const add = (extensions: string, info: KindInfo) => extensions.split(' ').forEach((e) => (BY_EXTENSION[e] = info))

add('png jpg jpeg gif webp avif bmp', kind(k.image, FileImage, tint(350), 'image'))
// The server sends SVG as a download only: it can hold script.
add('svg ico', kind(k.image, FileImage, tint(350), 'none'))
add('mp4 m4v webm mov', kind(k.video, FileVideo, tint(300), 'video'))
add('mkv avi', kind(k.video, FileVideo, tint(300), 'none'))
add('mp3 m4a wav ogg oga opus flac', kind(k.audio, FileMusic, tint(200), 'audio'))
add('pdf', kind(k.pdf, FileText, tint(25), 'pdf'))
add('zip gz tgz xz bz2 tar 7z zst rar deb rpm', kind(k.archive, FileArchive, tint(75), 'none'))
add('sh bash zsh fish', kind(k.script, FileTerminal, tint(150), 'text', 'shell'))
add('json js mjs ts tsx jsx py go rs c h cpp java rb php lua sql css html htm xml', kind(k.code, FileCode, tint(250), 'text', 'code'))
add('yml yaml toml ini conf cfg env service timer properties', kind(k.config, FileCog, tint(260), 'text', 'config'))
add('md markdown', kind(k.text, FileText, tint(230), 'text', 'markdown'))
add('txt log csv tsv', kind(k.text, FileText, tint(230), 'text'))

const BY_NAME: Record<string, KindInfo> = {
  dockerfile: BY_EXTENSION.sh,
  makefile: BY_EXTENSION.sh,
  license: BY_EXTENSION.txt,
  readme: BY_EXTENSION.txt,
}

/** Kind of a regular file, from its name alone. */
export function kindOfName(name: string): KindInfo {
  const lower = name.toLowerCase()
  const dot = lower.lastIndexOf('.')
  const byExtension = dot >= 0 ? BY_EXTENSION[lower.slice(dot + 1)] : undefined
  if (byExtension) return byExtension
  if (BY_NAME[lower]) return BY_NAME[lower]
  // Dot files such as ".bashrc" and ".env.local" are config text.
  if (lower.startsWith('.')) return BY_EXTENSION.conf
  return PLAIN_FILE
}

export function kindOf(entry: DirEntry): KindInfo {
  // A link opens as the thing it points to.
  const actual = entry.kind === 'symlink' ? entry.target : entry.kind
  if (actual === 'dir') return FOLDER
  if (actual === 'file') return kindOfName(entry.name)
  return entry.kind === 'symlink' ? BROKEN_LINK : SPECIAL
}
