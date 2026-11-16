// Starts `rackdeskd` on a test port with an empty data folder, and replaces
// the generated password with a known one. Returns the function that stops
// the server and deletes the folder.
import { spawn } from 'node:child_process'
import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { PORT } from '../playwright.config'
import { BASE_URL, PASSWORD, signInRequest } from './helpers'

export const BINARY = resolve(import.meta.dirname, '../../target/debug/rackdeskd')

export default async function globalSetup() {
  if (!existsSync(BINARY)) {
    throw new Error(`${BINARY} is missing. Run "cargo build -p rackdeskd" first.`)
  }
  const dataDir = mkdtempSync(join(tmpdir(), 'rackdesk-e2e-'))
  process.env.RD_E2E_DATA_DIR = dataDir

  const server = spawn(BINARY, ['--data-dir', dataDir, '--listen', `127.0.0.1:${PORT}`], {
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  const stop = () => {
    server.kill('SIGINT')
    rmSync(dataDir, { recursive: true, force: true })
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
