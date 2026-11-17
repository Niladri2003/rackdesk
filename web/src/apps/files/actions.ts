// What the file browser can do to an entry: new folder, rename, trash,
// delete, copy, cut, paste and duplicate. Each action asks the user where
// it must, calls the server, shows a message on failure, and tells open
// windows which folders changed.
import { api, ApiFailure, saveTextFile } from '../../api/client'
import type { Conflict } from '../../api/generated/Conflict'
import { errorText, t } from '../../i18n'
import { createStore, useStore } from '../../lib/store'
import { askText, choose } from '../../shell/dialog'
import { notify } from '../../shell/notify'
import { folderChanged, TRASH } from './changes'
import { baseName, joinPath, parentPath } from './paths'

const f = t.files

/** An entry that was cut or copied, waiting for a paste. It is shared by
 * all file browser windows. */
export interface Clip {
  mode: 'cut' | 'copy'
  path: string
}

const clipStore = createStore<Clip | null>(null)
export const useClip = () => useStore(clipStore)

export function setClip(mode: Clip['mode'], path: string): void {
  clipStore.set({ mode, path })
}

/** A name is 1 part of a path. The server checks this too. */
function checkName(name: string): string | null {
  if (name.includes('/')) return f.nameHasSlash
  if (name === '.' || name === '..') return f.nameReserved
  return null
}

function failed(e: unknown): void {
  // A cancelled request and a closed dialog are not errors to show.
  if (e instanceof ApiFailure && e.code === 'aborted') return
  notify('error', errorText(e))
}

/** Runs `op`. If the target name is in use, asks what to do and runs it
 * again with the answer. Resolves to null if the user chose nothing. */
export async function withConflict<T>(name: string, op: (conflict: Conflict) => Promise<T>): Promise<T | null> {
  try {
    return await op('fail')
  } catch (e) {
    if (!(e instanceof ApiFailure) || e.code !== 'already_exists') throw e
  }
  const answer = await choose<Conflict>(f.conflictTitle(name), f.conflictBody, [
    { label: f.keepBoth, value: 'keep_both' },
    { label: f.replace, value: 'replace', kind: 'danger' },
  ])
  return answer === null ? null : op(answer)
}

export async function newFolder(dir: string): Promise<void> {
  const name = await askText({ title: f.newFolder, label: f.newFolderLabel, initial: f.newFolderDefault, submit: f.create, check: checkName })
  if (name === null) return
  try {
    await api.fs.mkdir(joinPath(dir, name))
    folderChanged(dir)
  } catch (e) {
    failed(e)
  }
}

/** Makes an empty file and gives its path to `open`. The server refuses a
 * name that is in use, so an existing file is never emptied. */
export async function newFile(dir: string, open: (path: string) => void): Promise<void> {
  const name = await askText({ title: f.newFile, label: f.newFileLabel, initial: f.newFileDefault, submit: f.create, check: checkName })
  if (name === null) return
  const path = joinPath(dir, name)
  try {
    await saveTextFile(path, '', '')
    folderChanged(dir)
    open(path)
  } catch (e) {
    failed(e)
  }
}

export async function renameEntry(dir: string, name: string): Promise<void> {
  const next = await askText({ title: f.renameTitle(name), label: f.renameLabel, initial: name, submit: f.rename, check: checkName })
  if (next === null || next === name) return
  try {
    await api.fs.rename(joinPath(dir, name), joinPath(dir, next))
    folderChanged(dir)
  } catch (e) {
    failed(e)
  }
}

async function removeForGood(dir: string, name: string): Promise<void> {
  await api.fs.remove(joinPath(dir, name))
  folderChanged(dir)
}

export async function deleteForever(dir: string, name: string): Promise<void> {
  const ok = await choose(f.deleteTitle(name), f.deleteBody, [{ label: f.deleteForever, value: true, kind: 'danger' }])
  if (!ok) return
  try {
    await removeForGood(dir, name)
  } catch (e) {
    failed(e)
  }
}

export async function trashEntry(dir: string, name: string): Promise<void> {
  try {
    await api.fs.trash(joinPath(dir, name))
    folderChanged(dir)
    folderChanged(TRASH)
    notify('info', f.movedToTrash(name))
  } catch (e) {
    // The trash is on another disk, or can't be written. Offer the other way.
    const noTrash = e instanceof ApiFailure && (e.code === 'cross_device' || e.code === 'permission_denied')
    if (!noTrash) return failed(e)
    const ok = await choose(f.noTrashTitle(name), f.noTrashBody, [{ label: f.deleteForever, value: true, kind: 'danger' }])
    if (!ok) return
    try {
      await removeForGood(dir, name)
    } catch (again) {
      failed(again)
    }
  }
}

export async function duplicateEntry(dir: string, name: string): Promise<void> {
  const path = joinPath(dir, name)
  try {
    await api.fs.copy(path, path, 'keep_both')
    folderChanged(dir)
  } catch (e) {
    failed(e)
  }
}

/** Puts the cut or copied entry into `dir`. */
export async function pasteInto(dir: string): Promise<void> {
  const clip = clipStore.get()
  if (!clip) return
  const name = baseName(clip.path)
  const target = joinPath(dir, name)
  try {
    if (clip.mode === 'copy') {
      // A copy into the folder it is in is a duplicate: no question needed.
      const done = target === clip.path ? await api.fs.copy(clip.path, target, 'keep_both') : await withConflict(name, (c) => api.fs.copy(clip.path, target, c))
      if (done === null) return
    } else {
      if (target === clip.path) return
      const done = await withConflict(name, (c) => api.fs.rename(clip.path, target, c))
      if (done === null) return
      clipStore.set(null)
      folderChanged(parentPath(clip.path))
    }
    folderChanged(dir)
  } catch (e) {
    failed(e)
  }
}

export async function restoreFromTrash(id: string, name: string): Promise<void> {
  try {
    const done = await withConflict(name, (c) => api.fs.trashRestore(id, c))
    if (done === null) return
    folderChanged(TRASH)
    folderChanged(parentPath(done.path))
    notify('success', f.restored(done.path))
  } catch (e) {
    failed(e)
  }
}

export async function deleteFromTrash(id: string, name: string): Promise<void> {
  const ok = await choose(f.deleteTitle(name), f.deleteBody, [{ label: f.deleteForever, value: true, kind: 'danger' }])
  if (!ok) return
  try {
    await api.fs.trashDelete(id)
    folderChanged(TRASH)
  } catch (e) {
    failed(e)
  }
}

export async function emptyTrash(count: number): Promise<void> {
  const ok = await choose(f.emptyTrashTitle, f.emptyTrashBody(count), [{ label: f.emptyTrash, value: true, kind: 'danger' }])
  if (!ok) return
  try {
    await api.fs.trashEmpty()
    folderChanged(TRASH)
  } catch (e) {
    failed(e)
  }
}
