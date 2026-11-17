// Uploads. A file is sent in chunks. A lost connection, a pause or a page
// that sent a chunk twice does not start the file again: the upload goes on
// from the byte the server has.
import { api, ApiFailure, uploadChunk } from '../api/client'
import type { Conflict } from '../api/generated/Conflict'
import { folderChanged } from '../apps/files/changes'
import { errorText, t } from '../i18n'
import { createStore, useStore } from '../lib/store'
import { choose } from './dialog'

/** Bytes per chunk. The server accepts up to 8 MiB. */
const CHUNK_BYTES = 4 * 1024 * 1024
/** Files sent at the same time. The rest wait. */
const AT_ONCE = 2
/** When every upload has finished, the list is cleared after this time, so
 * the panel does not stay on top of other windows. */
const CLEAR_AFTER_MS = 6000

export type TransferState = 'waiting' | 'sending' | 'paused' | 'failed' | 'done'

export interface Transfer {
  id: number
  name: string
  /** The folder that receives the file. */
  dir: string
  size: number
  sent: number
  state: TransferState
  error: string | null
}

/** What the engine keeps for a transfer besides what is shown. */
interface Job {
  file: File
  conflict: Conflict
  /** The id the server gave, once the upload has begun. */
  uploadId: string | null
  abort: AbortController | null
}

const store = createStore<Transfer[]>([])
const jobs = new Map<number, Job>()
let nextId = 1

export const useTransfers = () => useStore(store)

function update(id: number, patch: Partial<Transfer>): void {
  store.set((list) => list.map((item) => (item.id === id ? { ...item, ...patch } : item)))
}

const find = (id: number) => store.get().find((item) => item.id === id)

/** Starts waiting transfers while fewer than `AT_ONCE` are being sent. */
function pump(): void {
  let sending = store.get().filter((item) => item.state === 'sending').length
  for (const item of store.get()) {
    if (sending >= AT_ONCE) return
    if (item.state === 'waiting') {
      sending++
      update(item.id, { state: 'sending', error: null })
      void run(item.id)
    }
  }
}

async function run(id: number): Promise<void> {
  const job = jobs.get(id)
  const transfer = find(id)
  if (!job || !transfer) return
  job.abort = new AbortController()
  try {
    let offset = transfer.sent
    if (job.uploadId === null) {
      const begun = await begin(transfer, job)
      if (begun === null) {
        // The user chose to skip this file.
        remove(id)
        return
      }
      offset = begun
    } else {
      // Resume: ask the server how much it has.
      offset = (await api.fs.uploadStatus(job.uploadId)).offset
    }
    update(id, { sent: offset })

    let done = transfer.size === 0
    while (!done) {
      const chunk = job.file.slice(offset, offset + CHUNK_BYTES)
      try {
        const status = await uploadChunk(job.uploadId!, offset, chunk, job.abort.signal)
        offset = status.offset
        done = status.path !== null
      } catch (e) {
        // The server has this chunk already. Go on from where it is.
        if (e instanceof ApiFailure && e.code === 'offset_mismatch' && e.uploadOffset !== null) offset = e.uploadOffset
        else throw e
      }
      update(id, { sent: offset })
    }
    update(id, { state: 'done', sent: transfer.size })
    jobs.delete(id)
    folderChanged(transfer.dir)
    setTimeout(() => {
      if (store.get().every((item) => item.state === 'done')) clearFinished()
    }, CLEAR_AFTER_MS)
  } catch (e) {
    const aborted = e instanceof ApiFailure && e.code === 'aborted'
    // The server forgot the upload (for example after a restart): the next
    // try starts it again.
    if (e instanceof ApiFailure && e.code === 'not_found') {
      job.uploadId = null
      update(id, { sent: 0 })
    }
    if (!aborted && find(id)) update(id, { state: 'failed', error: errorText(e) })
  } finally {
    job.abort = null
    pump()
  }
}

/** Tells the server about the file. Returns the byte to start from, or null
 * if the name is in use and the user chose to skip. */
async function begin(transfer: Transfer, job: Job): Promise<number | null> {
  for (;;) {
    try {
      const status = await api.fs.uploadBegin(transfer.dir, transfer.name, transfer.size, job.conflict)
      job.uploadId = status.id
      return status.offset
    } catch (e) {
      if (!(e instanceof ApiFailure) || e.code !== 'already_exists') throw e
      const answer = await choose<Conflict>(t.files.conflictTitle(transfer.name), t.files.conflictBody, [
        { label: t.files.keepBoth, value: 'keep_both' },
        { label: t.files.replace, value: 'replace', kind: 'danger' },
      ])
      if (answer === null) return null
      job.conflict = answer
    }
  }
}

function remove(id: number): void {
  jobs.delete(id)
  store.set((list) => list.filter((item) => item.id !== id))
}

/** Queues files for upload into `dir`. */
export function startUploads(files: File[], dir: string): void {
  const added = files.map((file): Transfer => {
    const id = nextId++
    jobs.set(id, { file, conflict: 'fail', uploadId: null, abort: null })
    return { id, name: file.name, dir, size: file.size, sent: 0, state: 'waiting', error: null }
  })
  store.set((list) => [...list, ...added])
  pump()
}

export function pauseTransfer(id: number): void {
  update(id, { state: 'paused' })
  jobs.get(id)?.abort?.abort()
}

/** Continues a paused or failed transfer. */
export function resumeTransfer(id: number): void {
  update(id, { state: 'waiting', error: null })
  pump()
}

/** Stops a transfer and removes the part the server has. */
export function cancelTransfer(id: number): void {
  const job = jobs.get(id)
  job?.abort?.abort()
  if (job?.uploadId) void api.fs.uploadCancel(job.uploadId).catch(() => undefined)
  remove(id)
  pump()
}

export function clearFinished(): void {
  store.set((list) => list.filter((item) => item.state !== 'done'))
}
