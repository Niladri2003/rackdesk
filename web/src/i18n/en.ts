// All text the UI shows, in English. This is the only file with UI text.
// To add a language, copy this file, translate the values, and select it in
// `index.ts`. Keys and function arguments must stay the same.

export const en = {
  product: 'RackDesk',

  common: {
    cancel: 'Cancel',
    close: 'Close',
    dismiss: 'Dismiss',
    retry: 'Retry now',
    connecting: 'Connecting...',
  },

  errors: {
    invalid_credentials: 'Wrong username or password.',
    wrong_password: 'The current password is wrong.',
    password_too_short: 'The new password must have at least 12 characters.',
    password_too_long: 'The new password must have at most 256 characters.',
    password_too_common: 'This password is on the list of common passwords. Choose another one.',
    password_unchanged: 'The new password must differ from the current one.',
    csrf_rejected: 'The server refused this request. Reload the page and try again.',
    unauthorized: 'Your session has ended. Sign in again.',
    network: 'The server did not answer.',
    rateLimited: (seconds: number | null) =>
      seconds === null
        ? 'Too many failed attempts. Try again later.'
        : `Too many failed attempts. Try again in ${seconds} s.`,
    unknown: (code: string) => `The request failed (${code}).`,
    generic: 'The request failed.',
  },

  signIn: {
    title: 'Sign in',
    username: 'Username',
    password: 'Password',
    showPassword: 'Show password',
    hidePassword: 'Hide password',
    submit: 'Sign in',
    busy: 'Signing in...',
    serverAt: (host: string) => `Server at ${host}`,
  },

  setPassword: {
    eyebrow: (user: string) => `First sign-in as ${user}`,
    title: 'Set a new password',
    body: 'You signed in with the password that the server printed at install. Replace it with one that only you know.',
    current: 'Printed password',
    next: 'New password',
    confirm: 'Confirm password',
    ruleLength: 'At least 12 characters',
    ruleMatch: 'Both passwords are the same',
    submit: 'Save and continue',
    busy: 'Saving...',
    done: 'Password set.',
  },

  rail: {
    label: 'Taskbar',
    launcher: 'Open search',
    launcherHint: (mod: string) => `Search (${mod}K)`,
    notifications: 'Notifications',
    unread: (n: number) => `Notifications, ${n} unread`,
    account: 'Account menu',
    time: 'Time',
  },

  window: {
    minimise: 'Minimise',
    maximise: 'Maximise',
    restore: 'Restore',
    close: 'Close',
    resizeRight: 'Resize from the right edge',
    resizeBottom: 'Resize from the bottom edge',
    resizeCorner: 'Resize from the corner',
    title: (app: string, detail: string) => (detail ? `${app} - ${detail}` : app),
  },

  switcher: {
    label: 'Switch windows',
    hintMove: 'Hold Alt and press ` to move',
    hintBack: 'Shift goes back',
    hintCommit: 'Release Alt to switch',
    hintCancel: 'Esc cancels',
  },

  search: {
    label: 'Search',
    inputLabel: 'Search everything',
    placeholder: 'Search apps, settings and actions',
    results: 'Results',
    groupApps: 'Apps',
    groupSettings: 'Settings',
    groupActions: 'Actions',
    groupSuggested: 'Suggested actions',
    later: 'Later',
    noneTitle: 'No results',
    noneBody: (q: string) => `Nothing named "${q}" in apps, settings or actions.`,
    move: 'Move',
    open: 'Open',
    close: 'Close',
    openHint: 'Open search anytime with',
    settingsPath: (section: string) => `Settings > ${section}`,
  },

  userMenu: {
    settings: 'Settings',
    signOut: 'Sign out',
  },

  notifications: {
    title: 'Notifications',
    clear: 'Clear all',
    empty: 'No notifications.',
    justNow: 'Just now',
    minutesAgo: (n: number) => `${n} min ago`,
    at: (time: string) => `At ${time}`,
  },

  connection: {
    lost: 'Connection to the server lost.',
    retryIn: (seconds: number) => `Retrying in ${seconds} s`,
    retrying: 'Retrying...',
    restored: 'Connection restored.',
  },

  apps: {
    files: { name: 'Files', description: 'Browse and manage files on the server' },
    terminal: { name: 'Terminal', description: 'A shell on the server' },
    settings: { name: 'Settings', description: 'Account, appearance and about' },
    notBuiltTitle: (app: string) => `${app} is not built yet`,
    notBuiltBody: 'This app is planned. The window is here so you can try moving, resizing and snapping.',
    webStub: 'Web apps in windows are planned for a later version.',
    streamStub: 'Streamed apps in windows are planned for a later version.',
  },

  actions: {
    signOut: { title: 'Sign out', detail: 'End this session' },
    changePassword: { title: 'Change password', detail: 'Settings > Account' },
    toggleTheme: { title: 'Switch between light and dark', detail: 'Settings > Appearance' },
  },

  settings: {
    sectionsLabel: 'Settings sections',
    account: {
      name: 'Account',
      lead: 'The account you sign in with.',
      role: 'Administrator',
      changeTitle: 'Change password',
      changeLead: 'Changing it signs you out in every other browser.',
      current: 'Current password',
      next: 'New password',
      hint: 'At least 12 characters. A short sentence works well.',
      confirm: 'Confirm new password',
      mismatch: 'The 2 new passwords are not the same.',
      submit: 'Change password',
      busy: 'Saving...',
      changed: 'Password changed. Other browsers were signed out.',
      signOutTitle: 'Sign out',
      signOutLead: 'End this session. Open windows close.',
    },
    appearance: {
      name: 'Appearance',
      lead: 'Changes apply at once and are kept in this browser.',
      theme: 'Theme',
      themeLead: 'System follows the light or dark setting of your device.',
      themeSystem: 'System',
      themeLight: 'Light',
      themeDark: 'Dark',
      accent: 'Accent colour',
      accents: { periwinkle: 'Periwinkle', clay: 'Clay', sage: 'Sage', ocean: 'Ocean', plum: 'Plum' },
      wallpaper: 'Wallpaper',
      wallpaperLead: 'The built-in tones follow your accent and theme.',
      wallpapers: { tinted: 'Tinted', sand: 'Sand', stone: 'Stone' },
    },
    about: {
      name: 'About',
      lead: 'This server and this version of RackDesk.',
      hostname: 'Server name',
      version: 'RackDesk version',
      licence: 'Licence',
      licenceValue: 'Apache-2.0',
    },
  },
}

export type Messages = typeof en
