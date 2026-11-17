import { expect, test } from '@playwright/test'
import { PASSWORD, expectAccessible, resetPassword, taskbar } from './helpers'

test('a wrong password shows an error and the right one opens the desktop', async ({ page }) => {
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible()

  await page.getByLabel('Password', { exact: true }).fill('not the password')
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(page.getByRole('alert')).toHaveText('Wrong username or password.')

  await page.getByLabel('Password', { exact: true }).fill(PASSWORD)
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(taskbar(page)).toBeVisible()
})

test('a generated password must be replaced before the desktop opens', async ({ page }) => {
  const generated = resetPassword()
  await page.goto('/')
  await page.getByLabel('Password', { exact: true }).fill(generated)
  await page.getByRole('button', { name: 'Sign in' }).click()

  await expect(page.getByRole('heading', { name: 'Set a new password' })).toBeVisible()
  const save = page.getByRole('button', { name: 'Save and continue' })
  await expect(save).toBeDisabled()

  await page.getByLabel('New password').fill(PASSWORD)
  await page.getByLabel('Confirm password').fill('something else')
  await expect(save).toBeDisabled()
  await page.getByLabel('Confirm password').fill(PASSWORD)
  await save.click()

  await expect(taskbar(page)).toBeVisible()
  await expect(page.getByRole('status').filter({ hasText: 'Password set.' })).toBeVisible()
})

test('after a reload the first-password page asks for the printed password', async ({ page }) => {
  const generated = resetPassword()
  await page.goto('/')
  await page.getByLabel('Password', { exact: true }).fill(generated)
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(page.getByRole('heading', { name: 'Set a new password' })).toBeVisible()
  await expect(page.getByLabel('Printed password')).toHaveCount(0)

  await page.reload()
  await page.getByLabel('Printed password').fill(generated)
  await page.getByLabel('New password').fill(PASSWORD)
  await page.getByLabel('Confirm password').fill(PASSWORD)
  await page.getByRole('button', { name: 'Save and continue' }).click()
  await expect(taskbar(page)).toBeVisible()
})

for (const scheme of ['light', 'dark'] as const) {
  test(`the sign-in page passes the accessibility checks in the ${scheme} theme`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme })
    await page.goto('/')
    await expect(page.getByRole('heading', { name: 'Sign in' })).toBeVisible()
    await expect(page.locator('html')).toHaveAttribute('data-theme', scheme)
    await expectAccessible(page)

    await page.getByLabel('Password', { exact: true }).fill('not the password')
    await page.getByRole('button', { name: 'Sign in' }).click()
    await expect(page.getByRole('alert')).toBeVisible()
    await expectAccessible(page)
  })
}
