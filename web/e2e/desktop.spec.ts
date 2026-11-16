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

test('a window resizes from its corner and keeps a minimum size', async ({ page }) => {
  await railButton(page, 'Terminal').click()
  const win = windowOf(page, 'Terminal')
  const before = await box(win)
  const corner = { x: before.x + before.width + 2, y: before.y + before.height + 2 }

  await drag(page, corner, { x: corner.x - 100, y: corner.y - 60 })
  const smaller = await box(win)
  expect(Math.round(before.width - smaller.width)).toBe(100)
  expect(Math.round(before.height - smaller.height)).toBe(60)

  const corner2 = { x: smaller.x + smaller.width + 2, y: smaller.y + smaller.height + 2 }
  await drag(page, corner2, { x: smaller.x + 50, y: smaller.y + 50 })
  const smallest = await box(win)
  expect(smallest.width).toBe(480)
  expect(smallest.height).toBe(320)
})

test('maximise fills the desktop and restore returns the old size', async ({ page }) => {
  await railButton(page, 'Files').click()
  const win = windowOf(page, 'Files')
  const before = await box(win)

  await win.getByRole('button', { name: 'Maximise' }).click()
  await expect.poll(async () => (await box(win)).width).toBe(1440 - RAIL_WIDTH)
  expect(await box(win)).toMatchObject({ x: RAIL_WIDTH, y: 0, height: 900 })

  await win.getByRole('button', { name: 'Restore' }).click()
  await expect.poll(async () => (await box(win)).width).toBe(before.width)
  expect(await box(win)).toMatchObject({ x: before.x, y: before.y })

  // A double click on the title bar does the same.
  const grip = await titleGrip(win)
  await page.mouse.dblclick(grip.x, grip.y)
  await expect(win.getByRole('button', { name: 'Restore' })).toBeVisible()
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

test('a click brings a window to the front', async ({ page }) => {
  await railButton(page, 'Files').click()
  await railButton(page, 'Terminal').click()
  const files = windowOf(page, 'Files')
  const terminal = windowOf(page, 'Terminal')
  await expect(terminal).toHaveAttribute('data-focused', 'true')

  const zIndex = (name: string) => windowOf(page, name).evaluate((el: HTMLElement) => Number(el.style.zIndex))
  expect(await zIndex('Terminal')).toBeGreaterThan(await zIndex('Files'))

  const b = await box(files)
  await page.mouse.click(b.x + 10, b.y + b.height - 10)
  await expect(files).toHaveAttribute('data-focused', 'true')
  expect(await zIndex('Files')).toBeGreaterThan(await zIndex('Terminal'))
})

test('Settings opens once, and several Files windows can be open', async ({ page }) => {
  await railButton(page, 'Settings').click()
  await page.keyboard.press('Control+k')
  await page.getByRole('combobox').fill('settings')
  await page.keyboard.press('Enter')
  await expect(windowOf(page, 'Settings')).toHaveCount(1)

  for (let i = 0; i < 2; i++) {
    await page.keyboard.press('Control+k')
    await page.getByRole('combobox').fill('files')
    await page.keyboard.press('Enter')
    await expect(windowOf(page, 'Files')).toHaveCount(i + 1)
  }
})

test('search finds apps, settings and actions, and works from the keyboard', async ({ page }) => {
  await page.keyboard.press('Control+k')
  const input = page.getByRole('combobox', { name: 'Search everything' })
  await expect(input).toBeFocused()
  // With no query it suggests apps and actions.
  await expect(page.getByRole('option', { name: /Files/ })).toContainText('Later')
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

test('Ctrl+Alt+arrows place the focused window from the keyboard', async ({ page }) => {
  await railButton(page, 'Files').click()
  const win = windowOf(page, 'Files')
  const half = (1440 - RAIL_WIDTH) / 2

  await page.keyboard.press('Control+Alt+ArrowRight')
  await expect.poll(async () => (await box(win)).x).toBe(RAIL_WIDTH + half)
  await page.keyboard.press('Control+Alt+ArrowUp')
  await expect.poll(async () => (await box(win)).width).toBe(1440 - RAIL_WIDTH)
  await page.keyboard.press('Control+Alt+ArrowDown')
  await expect.poll(async () => (await box(win)).width).toBe(1080)
  await page.keyboard.press('Control+Alt+ArrowDown')
  await expect(win).toHaveAttribute('data-min', 'true')
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

test('theme, accent and wallpaper change at once and are kept', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'light' })
  await railButton(page, 'Settings').click()
  const settings = windowOf(page, 'Settings')
  await settings.getByRole('button', { name: 'Appearance' }).click()
  const html = page.locator('html')
  const token = (name: string) => html.evaluate((el, n) => el.style.getPropertyValue(n), name)

  await settings.getByRole('radio', { name: 'Dark' }).click()
  await expect(html).toHaveAttribute('data-theme', 'dark')

  const accentBefore = await token('--acc')
  await settings.getByRole('radio', { name: 'Sage' }).click()
  expect(await token('--acc')).not.toBe(accentBefore)

  const wallBefore = await token('--wall')
  await settings.getByRole('radio', { name: 'Sand' }).click()
  expect(await token('--wall')).not.toBe(wallBefore)

  await page.reload()
  await expect(html).toHaveAttribute('data-theme', 'dark')
  await expect(windowOf(page, 'Settings').getByRole('radio', { name: 'Sage' })).toBeChecked()
})

test('About shows the server name and version', async ({ page }) => {
  await railButton(page, 'Settings').click()
  const settings = windowOf(page, 'Settings')
  await settings.getByRole('button', { name: 'About' }).click()
  await expect(settings.getByText(/^\d+\.\d+\.\d+$/)).toBeVisible()
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

test('on a narrow screen every window fills the desktop', async ({ page }) => {
  await page.setViewportSize({ width: 820, height: 1000 })
  await railButton(page, 'Files').click()
  const win = windowOf(page, 'Files')
  await expect.poll(async () => (await box(win)).width).toBe(820 - RAIL_WIDTH)
  await expect(win.getByRole('button', { name: 'Maximise' })).toHaveCount(0)
  await expect(win.getByRole('button', { name: 'Close' })).toBeVisible()
})

for (const scheme of ['light', 'dark'] as const) {
  test(`the desktop passes the accessibility checks in the ${scheme} theme`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme })
    await expect(page.locator('html')).toHaveAttribute('data-theme', scheme)

    // Empty desktop.
    await expectAccessible(page)

    // Windows: a placeholder app and each Settings section.
    await railButton(page, 'Files').click()
    await railButton(page, 'Settings').click()
    const settings = windowOf(page, 'Settings')
    for (const section of ['Account', 'Appearance', 'About']) {
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
