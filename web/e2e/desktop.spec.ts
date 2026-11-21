import { expect, test } from '@playwright/test'
import { box, drag, expectAccessible, openDesktop, railButton, taskbar, titleGrip, windowOf } from './helpers'

const RAIL_WIDTH = 64

test.beforeEach(async ({ page }) => {
  await openDesktop(page)
})

test('a window opens from the taskbar, moves by its title bar, and closes', async ({ page }) => {
  await railButton(page, 'Files').click()
  const win = windowOf(page, 'Files')
  await expect(win).toBeVisible()
  await expect(win).toBeFocused()

  const before = await box(win)
  const grip = await titleGrip(win)
  await drag(page, grip, { x: grip.x + 120, y: grip.y + 80 })
  const after = await box(win)
  expect(Math.round(after.x - before.x)).toBe(120)
  expect(Math.round(after.y - before.y)).toBe(80)
  expect(after.width).toBe(before.width)

  await win.getByRole('button', { name: 'Close' }).click()
  await expect(win).toHaveCount(0)
})

test('moving a window does not redraw the other windows, the apps or the taskbar', async ({ page }) => {
  // Turn on the draw counters (src/lib/renderCount.ts) before the page loads.
  await page.addInitScript(() => {
    window.__rdRenderCounts = {}
  })
  await page.reload()
  await railButton(page, 'Settings').click()
  await railButton(page, 'Files').click()
  const files = windowOf(page, 'Files')
  await expect(files).toBeFocused()
  await box(files)

  await page.evaluate(() => {
    window.__rdRenderCounts = {}
  })
  const grip = await titleGrip(files)
  await page.mouse.move(grip.x, grip.y)
  await page.mouse.down()
  for (let i = 1; i <= 20; i++) await page.mouse.move(grip.x + i * 5, grip.y + i * 3)
  await page.mouse.up()

  const counts = await page.evaluate(() => window.__rdRenderCounts ?? {})
  // The moved window draws once per pointer move.
  expect(counts['window:files']).toBeGreaterThanOrEqual(15)
  // Nothing else draws at all.
  expect(counts).toEqual({ 'window:files': counts['window:files'] })

  // A resize also leaves the app content alone.
  await page.evaluate(() => {
    window.__rdRenderCounts = {}
  })
  const b = await box(files)
  const corner = { x: b.x + b.width + 2, y: b.y + b.height + 2 }
  await drag(page, corner, { x: corner.x - 60, y: corner.y - 40 })
  const after = await page.evaluate(() => window.__rdRenderCounts ?? {})
  expect(Object.keys(after)).toEqual(['window:files'])
})

test('dragging to a side edge snaps the window to that half', async ({ page }) => {
  await railButton(page, 'Files').click()
  const win = windowOf(page, 'Files')
  const grip = await titleGrip(win)
  const half = (1440 - RAIL_WIDTH) / 2

  await page.mouse.move(grip.x, grip.y)
  await page.mouse.down()
  await page.mouse.move(RAIL_WIDTH + 300, 200, { steps: 4 })
  await page.mouse.move(RAIL_WIDTH + 3, 200, { steps: 4 })
  await expect(page.locator('.snap-preview')).toBeVisible()
  await page.mouse.up()
  await expect(page.locator('.snap-preview')).toHaveCount(0)
  await expect.poll(async () => (await box(win)).width).toBe(half)
  expect(await box(win)).toMatchObject({ x: RAIL_WIDTH, y: 0, height: 900 })

  // Dragging it away restores the size it had before.
  const grip2 = await titleGrip(win)
  await drag(page, grip2, { x: 700, y: 300 })
  await expect.poll(async () => (await box(win)).width).toBe(1080)
})

test('the taskbar button minimises the focused window and brings it back', async ({ page }) => {
  const files = railButton(page, 'Files')
  await files.click()
  const win = windowOf(page, 'Files')
  await expect(files).toHaveAttribute('data-state', 'focused')

  await files.click()
  await expect(win).toHaveAttribute('data-min', 'true')
  await expect(win).toHaveAttribute('inert', '')
  await expect(files).toHaveAttribute('data-state', 'open')

  await files.click()
  await expect(win).toHaveAttribute('data-min', 'false')
  await expect(win).toBeFocused()
})

test('search finds apps, settings and actions, and works from the keyboard', async ({ page }) => {
  await page.keyboard.press('Control+k')
  const input = page.getByRole('combobox', { name: 'Search everything' })
  await expect(input).toBeFocused()
  // With no query it suggests apps and actions.
  await expect(page.getByRole('option', { name: /Terminal/ })).toBeVisible()
  await expect(page.getByRole('option', { name: /Sign out/ })).toBeVisible()

  await input.fill('wallpaper')
  const option = page.getByRole('option')
  await expect(option).toHaveCount(1)
  await expect(option).toHaveAttribute('aria-selected', 'true')
  await page.keyboard.press('Enter')

  const settings = windowOf(page, 'Settings')
  await expect(settings).toBeVisible()
  await expect(settings).toHaveAttribute('aria-label', 'Settings - Appearance')
  // Focus moves into the window that the action opened.
  await expect(settings).toBeFocused()

  await page.keyboard.press('Control+k')
  await input.fill('zzzz')
  await expect(page.getByText('No results')).toBeVisible()
  await page.keyboard.press('Escape')
  await expect(input).toHaveCount(0)
})

test('the terminal runs a command on the server, and Activity lists it', async ({ page }) => {
  await railButton(page, 'Terminal').click()
  const win = windowOf(page, 'Terminal')
  // The text copy of the screen that the terminal keeps for screen readers.
  const screen = win.locator('.xterm-accessibility-tree')
  await expect(screen).toBeAttached()
  await win.getByRole('textbox', { name: 'Terminal input' }).focus()
  // The sum shows that a shell ran the line, not only that it was typed.
  await page.keyboard.type('echo web-$((6*7))')
  await page.keyboard.press('Enter')
  await expect(screen).toContainText('web-42')

  // Ctrl+K belongs to the shell here. It must not open the search.
  await page.keyboard.press('Control+k')
  await expect(page.getByRole('combobox')).toHaveCount(0)

  await win.getByRole('button', { name: 'Close' }).click()
  await railButton(page, 'Settings').click()
  const settings = windowOf(page, 'Settings')
  await settings.getByRole('button', { name: 'Activity' }).click()
  const table = settings.getByRole('table')
  await expect(table.getByRole('row').filter({ hasText: 'terminal.open' }).first()).toBeVisible()
  await expect(table.getByRole('row').filter({ hasText: 'terminal.close' }).first()).toBeVisible()
  // What was typed is not in the log.
  await expect(table).not.toContainText('web-')
})

test('Alt+` switches to the window that was used before', async ({ page }) => {
  await railButton(page, 'Files').click()
  await railButton(page, 'Terminal').click()
  await expect(windowOf(page, 'Terminal')).toHaveAttribute('data-focused', 'true')

  await page.keyboard.down('Alt')
  await page.keyboard.press('Backquote')
  const switcher = page.getByRole('listbox', { name: 'Switch windows' })
  await expect(switcher).toBeVisible()
  await expect(switcher.getByRole('option', { selected: true })).toContainText('Files')
  await page.keyboard.up('Alt')

  await expect(switcher).toHaveCount(0)
  await expect(windowOf(page, 'Files')).toHaveAttribute('data-focused', 'true')
})

test('the window layout is restored after a reload', async ({ page }) => {
  await railButton(page, 'Files').click()
  const win = windowOf(page, 'Files')
  const grip = await titleGrip(win)
  await drag(page, grip, { x: grip.x - 150, y: grip.y + 60 })
  const before = await box(win)
  await railButton(page, 'Settings').click()
  await windowOf(page, 'Settings').getByRole('button', { name: 'About' }).click()

  await page.reload()
  await expect(windowOf(page, 'Settings')).toHaveAttribute('aria-label', 'Settings - About')
  expect(await box(windowOf(page, 'Files'))).toMatchObject({ x: before.x, y: before.y, width: before.width })
})

test('a message shows as a toast and stays in the notification list', async ({ page }) => {
  await page.keyboard.press('Control+k')
  await page.getByRole('combobox').fill('light and dark')
  await page.keyboard.press('Enter')
  // No message for a theme change. Lose the connection to get one.
  await page.route('**/healthz', (route) => route.abort())
  await page.route('**/api/v1/server', (route) => route.abort())
  await railButton(page, 'Settings').click()
  await windowOf(page, 'Settings').getByRole('button', { name: 'About' }).click()

  const banner = page.getByRole('status').filter({ hasText: 'Connection to the server lost.' })
  await expect(banner).toBeVisible()

  await page.unroute('**/healthz')
  await page.unroute('**/api/v1/server')
  await banner.getByRole('button', { name: 'Retry now' }).click()
  await expect(banner).toHaveCount(0)
  await expect(page.locator('.toast')).toHaveText(/Connection restored\./)

  const bell = taskbar(page).getByRole('button', { name: /Notifications/ })
  await expect(bell).toHaveAccessibleName('Notifications, 1 unread')
  await bell.click()
  const list = page.getByRole('dialog', { name: 'Notifications' })
  await expect(list.getByText('Connection restored.')).toBeVisible()
  await expect(bell).toHaveAccessibleName('Notifications')
  await list.getByRole('button', { name: 'Clear all' }).click()
  await expect(list.getByText('No notifications.')).toBeVisible()
})

for (const scheme of ['light', 'dark'] as const) {
  test(`the desktop passes the accessibility checks in the ${scheme} theme`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme })
    await expect(page.locator('html')).toHaveAttribute('data-theme', scheme)

    // Empty desktop.
    await expectAccessible(page)

    // Windows: a placeholder app and each Settings section. Files has a
    // test of its own in files.spec.ts.
    await railButton(page, 'Terminal').click()
    await railButton(page, 'Settings').click()
    const settings = windowOf(page, 'Settings')
    for (const section of ['Account', 'Appearance', 'Activity', 'About']) {
      await settings.getByRole('button', { name: section, exact: true }).click()
      await expect(settings.getByRole('heading', { name: section })).toBeVisible()
      await expectAccessible(page)
    }

    // Search, with results and with none.
    await page.keyboard.press('Control+k')
    await expect(page.getByRole('combobox')).toBeFocused()
    await expectAccessible(page)
    await page.getByRole('combobox').fill('zzzz')
    await expectAccessible(page)
    await page.keyboard.press('Escape')

    // Account menu and notifications.
    await taskbar(page).getByRole('button', { name: 'Account menu' }).click()
    await expect(page.getByRole('menu')).toBeVisible()
    await expectAccessible(page)
    await page.keyboard.press('Escape')
    await taskbar(page).getByRole('button', { name: /Notifications/ }).click()
    await expect(page.getByRole('dialog', { name: 'Notifications' })).toBeVisible()
    await expectAccessible(page)
    await page.keyboard.press('Escape')

    // Window switcher.
    await page.keyboard.down('Alt')
    await page.keyboard.press('Backquote')
    await expect(page.getByRole('listbox', { name: 'Switch windows' })).toBeVisible()
    await expectAccessible(page)
    await page.keyboard.up('Alt')
  })
}
