import { expect, test } from '@playwright/test'
import type { Page } from '@playwright/test'
import { BIG_FOLDER_SIZE, box, drag, expectAccessible, openDesktop, railButton, windowOf } from './helpers'

// The files root of the test server is made in global-setup.ts.

async function openFiles(page: Page) {
  await railButton(page, 'Files').click()
  const win = windowOf(page, 'Files')
  await expect(win.locator('.files-scroll')).toHaveAttribute('aria-busy', 'false')
  return win
}

async function goTo(page: Page, win: ReturnType<typeof windowOf>, path: string) {
  await win.getByRole('button', { name: 'Type a path' }).click()
  await win.getByLabel('Folder path').fill(path)
  await page.keyboard.press('Enter')
  await expect(win.locator('.files-scroll')).toHaveAttribute('aria-busy', 'false')
}

test.beforeEach(async ({ page }) => {
  await openDesktop(page)
})

test('folders open by double click, and back, forward and up move between them', async ({ page }) => {
  const win = await openFiles(page)
  await win.getByRole('option', { name: 'docs' }).dblclick()
  await expect(win).toHaveAttribute('aria-label', 'Files - docs')
  await win.getByRole('option', { name: 'deep' }).dblclick()
  await expect(win.getByRole('option', { name: /compose-stack\.yml/ })).toBeVisible()
  await expect(win.getByRole('navigation', { name: 'Path' }).getByRole('button', { name: 'deep' })).toHaveAttribute('aria-current', 'page')

  await win.getByRole('button', { name: 'Back' }).click()
  await expect(win).toHaveAttribute('aria-label', 'Files - docs')
  await win.getByRole('button', { name: 'Forward' }).click()
  await expect(win).toHaveAttribute('aria-label', 'Files - deep')
  await win.getByRole('button', { name: 'Up to the parent folder' }).click()
  await expect(win).toHaveAttribute('aria-label', 'Files - docs')

  await win.getByRole('navigation', { name: 'Path' }).getByRole('button', { name: '/' }).click()
  await expect(win).toHaveAttribute('aria-label', 'Files - /')
  await expect(win.getByRole('button', { name: 'Up to the parent folder' })).toBeDisabled()
})

test('a folder with thousands of files draws only the visible rows', async ({ page }) => {
  const win = await openFiles(page)
  await win.getByRole('option', { name: 'big' }).dblclick()
  await expect(win.getByRole('status')).toHaveText(`${BIG_FOLDER_SIZE.toLocaleString('en')} items`)
  expect(await win.locator('.files-row').count()).toBeLessThan(80)

  // The End key jumps to the last file. Its page is loaded on demand.
  await win.getByRole('listbox').focus()
  await page.keyboard.press('End')
  const lastName = `file-${String(BIG_FOLDER_SIZE - 1).padStart(5, '0')}.log`
  const last = win.getByRole('option', { name: lastName })
  await expect(last).toBeVisible()
  await expect(last).toHaveAttribute('aria-selected', 'true')
  await expect(last).toHaveAttribute('aria-posinset', String(BIG_FOLDER_SIZE))
  expect(await win.locator('.files-row').count()).toBeLessThan(80)

  await page.keyboard.press('Home')
  await expect(win.getByRole('option', { name: 'file-00000.log' })).toHaveAttribute('aria-selected', 'true')
})

test('the list works from the keyboard', async ({ page }) => {
  const win = await openFiles(page)
  await win.getByRole('listbox').focus()
  await page.keyboard.press('ArrowDown')
  await page.keyboard.press('ArrowDown')
  await expect(win.getByRole('option', { name: 'docs' })).toHaveAttribute('aria-selected', 'true')
  await page.keyboard.press('Enter')
  await expect(win).toHaveAttribute('aria-label', 'Files - docs')
  await win.getByRole('listbox').focus()
  await page.keyboard.press('Backspace')
  await expect(win).toHaveAttribute('aria-label', 'Files - /')
})

test('a text file opens in a preview window with line numbers', async ({ page }) => {
  const win = await openFiles(page)
  await win.getByRole('option', { name: 'docs' }).dblclick()
  await win.getByRole('option', { name: /guide\.md/ }).dblclick()

  const preview = windowOf(page, 'Preview')
  await expect(preview).toHaveAttribute('aria-label', 'Preview - guide.md')
  await expect(preview.locator('.preview-line').first()).toHaveText('1# Guide')
  await expect(preview.locator('.tok-heading')).toHaveText('# Guide')
  await expect(preview.getByText('3 lines')).toBeVisible()
  // A preview has a button on the taskbar while it is open.
  await expect(railButton(page, 'Preview - guide.md')).toHaveAttribute('data-state', 'focused')

  await preview.getByRole('button', { name: 'Show in Files' }).click()
  await expect(windowOf(page, 'Files').last()).toHaveAttribute('aria-label', 'Files - docs')
})

test('an image opens in the preview', async ({ page }) => {
  const win = await openFiles(page)
  await goTo(page, win, '/photos')
  await win.getByRole('option', { name: /pixel\.png/ }).dblclick()
  const image = windowOf(page, 'Preview').getByRole('img', { name: 'pixel.png' })
  await expect(image).toBeVisible()
  await expect.poll(() => image.evaluate((el: HTMLImageElement) => el.naturalWidth)).toBe(1)
})

test('an HTML file is shown as text and its script does not run', async ({ page }) => {
  const win = await openFiles(page)
  await win.getByRole('option', { name: /page\.html/ }).dblclick()
  const preview = windowOf(page, 'Preview')
  await expect(preview.locator('.preview-line').first()).toContainText('<h1 id="injected">not a page</h1>')
  expect(await page.locator('#injected').count()).toBe(0)
  expect(await page.evaluate(() => (window as unknown as { __pwned?: boolean }).__pwned)).toBeUndefined()

  // Asked for directly, the server still sends it as a download.
  const res = await page.request.get('/api/v1/fs/download?path=/page.html&inline=true')
  expect(res.headers()['content-type']).toBe('application/octet-stream')
  expect(res.headers()['content-disposition']).toMatch(/^attachment/)
})

test('the one search finds files by name and opens them', async ({ page }) => {
  await page.keyboard.press('Control+k')
  await page.getByRole('combobox').fill('compose')
  const group = page.getByRole('group', { name: 'Files' })
  const hit = group.getByRole('option', { name: /compose-stack\.yml/ })
  await expect(hit).toContainText('/docs/deep/compose-stack.yml')
  await hit.click()
  await expect(windowOf(page, 'Preview')).toHaveAttribute('aria-label', 'Preview - compose-stack.yml')

  // A folder opens in Files.
  await page.keyboard.press('Control+k')
  await page.getByRole('combobox').fill('photos')
  await page.getByRole('group', { name: 'Files' }).getByRole('option', { name: /photos/ }).click()
  await expect(windowOf(page, 'Files')).toHaveAttribute('aria-label', 'Files - photos')
})

test('the grid view lays tiles out in rows and columns, and follows the window width', async ({ page }) => {
  const win = await openFiles(page)
  await win.getByRole('radio', { name: 'Grid' }).click()
  await expect(win.locator('.files-scroll')).toHaveAttribute('data-view', 'grid')

  // Position of each tile, relative to the first one.
  const layout = async () => {
    const boxes = await win.locator('.files-row').evaluateAll((els) => els.map((el) => el.getBoundingClientRect()).map((r) => ({ x: r.x, y: r.y, w: r.width, h: r.height })))
    return boxes.map((b) => ({ col: Math.round((b.x - boxes[0].x) / b.w), row: Math.round((b.y - boxes[0].y) / b.h) }))
  }
  // Tiles fill a row from the left, then start the next row.
  const columnsNow = async () => (await layout()).filter((tile) => tile.row === 0).length
  const isGrid = async () => {
    const tiles = await layout()
    const columns = tiles.filter((tile) => tile.row === 0).length
    return tiles.length === 8 && tiles.every((tile, i) => tile.col === i % columns && tile.row === Math.floor(i / columns))
  }
  await expect.poll(isGrid).toBe(true)
  const wide = await columnsNow()
  expect(wide).toBeGreaterThan(3)

  // A narrower window has fewer columns.
  const b = await box(win)
  await drag(page, { x: b.x + b.width + 2, y: b.y + b.height + 2 }, { x: b.x + 660, y: b.y + b.height + 2 })
  await expect.poll(columnsNow).toBeLessThan(wide)
  await expect.poll(isGrid).toBe(true)
})

for (const scheme of ['light', 'dark'] as const) {
  test(`Files and Preview pass the accessibility checks in the ${scheme} theme`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme })
    const win = await openFiles(page)
    await win.getByRole('option', { name: /notes\.txt/ }).click()
    await expectAccessible(page)

    await win.getByRole('radio', { name: 'Grid' }).click()
    await expect(win.locator('.files-scroll')).toHaveAttribute('data-view', 'grid')
    await expectAccessible(page)
    await win.getByRole('radio', { name: 'List' }).click()

    await goTo(page, win, '/empty')
    await expectAccessible(page)
    await goTo(page, win, '/nope')
    await expect(win.getByRole('alert')).toBeVisible()
    await expectAccessible(page)

    await goTo(page, win, '/docs')
    await win.getByRole('option', { name: /config\.yml/ }).dblclick()
    const preview = windowOf(page, 'Preview')
    await expect(preview.locator('.tok-key').first()).toBeVisible()
    // Maximised, so it does not cover a part of a button in the window
    // behind it. axe counts a half-covered button as too small a target.
    await preview.getByRole('button', { name: 'Maximise' }).click()
    await expect(preview).toHaveAttribute('data-filled', 'true')
    await expectAccessible(page)

    await page.keyboard.press('Control+k')
    await page.getByRole('combobox').fill('compose')
    await expect(page.getByRole('group', { name: 'Files' })).toBeVisible()
    await expectAccessible(page)

    // A picture, with its zoom buttons.
    await page.getByRole('combobox').fill('pixel')
    await page.getByRole('group', { name: 'Files' }).getByRole('option', { name: /pixel\.png/ }).click()
    const picture = windowOf(page, 'Preview').last()
    await expect(picture.getByRole('toolbar', { name: 'Zoom' })).toBeVisible()
    await picture.getByRole('button', { name: 'Maximise' }).click()
    await expect(picture).toHaveAttribute('data-filled', 'true')
    await expectAccessible(page)
  })
}
