// Starts `rackdeskd` on a test port with an empty data folder, and replaces
// the generated password with a known one. Returns the function that stops
// the server and deletes the folder.
import { spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { PORT } from '../playwright.config'
import { BASE_URL, BIG_FOLDER_SIZE, PASSWORD, signInRequest } from './helpers'

export const BINARY = resolve(import.meta.dirname, '../../target/debug/rackdeskd')

export default async function globalSetup() {
  if (!existsSync(BINARY)) {
    throw new Error(`${BINARY} is missing. Run "cargo build -p rackdeskd" first.`)
  }
  const dataDir = mkdtempSync(join(tmpdir(), 'rackdesk-e2e-'))
  process.env.RD_E2E_DATA_DIR = dataDir
  // The file browser sees only this folder. It is apart from the data folder,
  // so the account file is not in it.
  const filesRoot = mkdtempSync(join(tmpdir(), 'rackdesk-e2e-files-'))
  makeFiles(filesRoot)
  process.env.RD_E2E_FILES_ROOT = filesRoot

  const server = spawn(BINARY, ['--data-dir', dataDir, '--files-root', filesRoot, '--listen', `127.0.0.1:${PORT}`, '--terminal', 'direct'], {
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  const stop = () => {
    server.kill('SIGINT')
    rmSync(dataDir, { recursive: true, force: true })
    rmSync(filesRoot, { recursive: true, force: true })
  }

  try {
    // The server prints the generated password once, at first start.
    const generated = await new Promise<string>((done, fail) => {
      let output = ''
      const timer = setTimeout(() => fail(new Error(`no password in the server output:\n${output}`)), 15000)
      server.stdout.on('data', (chunk: Buffer) => {
        output += chunk.toString()
        const match = /password: (\S+)/.exec(output)
        if (match) {
          clearTimeout(timer)
          done(match[1])
        }
      })
      server.on('exit', (code) => fail(new Error(`the server stopped with code ${code}:\n${output}`)))
    })
    await waitUntilUp()

    const signIn = await signInRequest(generated)
    const cookie = (signIn.headers.get('set-cookie') ?? '').split(';')[0]
    const change = await fetch(`${BASE_URL}/api/v1/session/password`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Origin: BASE_URL, Cookie: cookie },
      body: JSON.stringify({ current_password: generated, new_password: PASSWORD }),
    })
    if (!change.ok) throw new Error(`could not set the test password: ${change.status}`)
  } catch (e) {
    stop()
    throw e
  }
  return stop
}

/** The files that the file browser tests expect. */
function makeFiles(root: string): void {
  mkdirSync(join(root, 'docs/deep'), { recursive: true })
  mkdirSync(join(root, 'photos'))
  mkdirSync(join(root, 'big'))
  mkdirSync(join(root, 'empty'))
  writeFileSync(join(root, 'docs/guide.md'), '# Guide\n\nHello from the guide.\n')
  writeFileSync(join(root, 'docs/config.yml'), '# settings\nport: 8080\nname: "atlas"\n')
  writeFileSync(join(root, 'docs/deep/compose-stack.yml'), 'services: {}\n')
  writeFileSync(join(root, 'notes.txt'), 'first line\nsecond line\n')
  writeFileSync(join(root, 'large.txt'), 'x'.repeat(5000))
  writeFileSync(join(root, '.hidden-file'), 'hidden\n')
  writeFileSync(join(root, 'page.html'), '<h1 id="injected">not a page</h1><script>window.__pwned = true</script>\n')
  writeFileSync(join(root, 'blob.bin'), Buffer.from([0, 1, 2, 3, 0, 255]))
  // A valid PNG image of 1 x 1 pixel.
  writeFileSync(
    join(root, 'photos/pixel.png'),
    Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64'),
  )
  for (let i = 0; i < BIG_FOLDER_SIZE; i++) {
    writeFileSync(join(root, 'big', `file-${String(i).padStart(5, '0')}.log`), '')
  }
}

async function waitUntilUp(): Promise<void> {
  for (let i = 0; i < 50; i++) {
    try {
      if ((await fetch(`${BASE_URL}/healthz`)).ok) return
    } catch {
      // Not listening yet.
    }
    await new Promise((r) => setTimeout(r, 100))
  }
  throw new Error('the server did not start')
}
