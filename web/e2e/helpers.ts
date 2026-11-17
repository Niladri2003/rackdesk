// Shared by the end-to-end tests.
import { spawnSync } from 'node:child_process'
import { resolve } from 'node:path'
import AxeBuilder from '@axe-core/playwright'
import { expect } from '@playwright/test'
import type { Locator, Page } from '@playwright/test'
import { PORT } from '../playwright.config'

export const BASE_URL = `http://127.0.0.1:${PORT}`
/** The password of the test account after global setup. It opens nothing
 * outside the temporary test server. */
export const PASSWORD = 'e2e test phrase for rackdesk'
/** Number of files in the `big` folder of the test files root. */
export const BIG_FOLDER_SIZE = 3000

const BINARY = resolve(import.meta.dirname, '../../target/debug/rackdeskd')

export function signInRequest(password: string): Promise<Response> {
  return fetch(`${BASE_URL}/api/v1/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Origin: BASE_URL },
    body: JSON.stringify({ username: 'admin', password }),
  })
}

/** Signs in through the API and opens the desktop with no windows. */
export async function openDesktop(page: Page): Promise<void> {
  const res = await page.request.post('/api/v1/session', {
    headers: { Origin: BASE_URL },
    data: { username: 'admin', password: PASSWORD },
  })
  expect(res.status()).toBe(200)
  await page.goto('/')
  await expect(taskbar(page)).toBeVisible()
}

/** Runs `rackdeskd reset-password` and returns the new generated password. */
export function resetPassword(): string {
  const dataDir = process.env.RD_E2E_DATA_DIR
  if (!dataDir) throw new Error('RD_E2E_DATA_DIR is not set')
  const result = spawnSync(BINARY, ['--data-dir', dataDir, 'reset-password'], { encoding: 'utf8' })
  const match = /password: (\S+)/.exec(result.stdout)
  if (!match) throw new Error(`reset-password printed no password:\n${result.stdout}${result.stderr}`)
  return match[1]
}

export const taskbar = (page: Page) => page.getByRole('navigation', { name: 'Taskbar' })
export const railButton = (page: Page, app: string) => taskbar(page).getByRole('button', { name: app, exact: true })
/** A window, found by the start of its accessible name. */
export const windowOf = (page: Page, app: string) => page.locator(`section.win[aria-label^="${app}"]`)

/** The box of an element, measured after its animations have ended. */
export async function box(locator: Locator) {
  await locator.evaluate((el) => Promise.all(el.getAnimations().map((a) => a.finished.catch(() => undefined))))
  const b = await locator.boundingBox()
  if (!b) throw new Error('the element has no box')
  return b
}

/** Drags from a point on the page to another, in steps, as a person does. */
export async function drag(page: Page, from: { x: number; y: number }, to: { x: number; y: number }) {
  await page.mouse.move(from.x, from.y)
  await page.mouse.down()
  await page.mouse.move((from.x + to.x) / 2, (from.y + to.y) / 2, { steps: 4 })
  await page.mouse.move(to.x, to.y, { steps: 4 })
  await page.mouse.up()
}

/** The point on a title bar that is safe to grab: left of the buttons. */
export async function titleGrip(win: Locator) {
  const b = await box(win.locator('.win-title'))
  return { x: b.x + b.width / 2, y: b.y + b.height / 2 }
}

/** Fails if the page breaks a WCAG 2.2 A or AA rule that axe can check. */
export async function expectAccessible(page: Page): Promise<void> {
  const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa']).analyze()
  const summary = results.violations.map(
    (v) => `${v.id}: ${v.help}\n${v.nodes.map((n) => `    ${n.target.join(' ')}  ${n.failureSummary?.split('\n')[1]?.trim() ?? ''}`).join('\n')}`,
  )
  expect(summary, summary.join('\n')).toEqual([])
}
