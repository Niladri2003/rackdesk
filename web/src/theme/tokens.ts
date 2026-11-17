// Design tokens as CSS variables. The values come from the accepted design
// (docs/design/). Colours are OKLCH. Neutrals have hue 260. The accent hue
// and the wallpaper tone are user settings.

export const ACCENTS = { periwinkle: 275, clay: 42, sage: 160, ocean: 230, plum: 330 } as const
export type Accent = keyof typeof ACCENTS
export const WALLPAPERS = ['tinted', 'sand', 'stone'] as const
export type Wallpaper = (typeof WALLPAPERS)[number]

const NEUTRAL_HUE = 260
const NEUTRAL_CHROMA = 0.012
const ACCENT_CHROMA = 0.15

/** Fixed colour for an accent swatch, the same in both themes. */
export const accentSwatch = (accent: Accent) => `oklch(0.6 ${ACCENT_CHROMA} ${ACCENTS[accent]})`

/** The wallpaper base colour for a theme, accent and tone. */
export function wallpaperColor(dark: boolean, accent: Accent, wall: Wallpaper): string {
  const hue = wall === 'sand' ? 70 : wall === 'stone' ? NEUTRAL_HUE : ACCENTS[accent]
  const chroma = wall === 'stone' ? 0.01 : wall === 'sand' ? 0.03 : 0.035
  return dark ? `oklch(0.27 ${chroma + 0.005} ${hue})` : `oklch(0.86 ${chroma} ${hue})`
}

export function makeTokens(dark: boolean, accent: Accent, wall: Wallpaper): Record<string, string> {
  const nh = NEUTRAL_HUE
  const ah = ACCENTS[accent]
  const n = (l: number, k = 1) => `oklch(${l} ${(NEUTRAL_CHROMA * k).toFixed(4)} ${nh})`
  const a = (l: number, c: number) => `oklch(${l} ${c.toFixed(4)} ${ah})`
  const ac = ACCENT_CHROMA
  const shared = { '--wall': wallpaperColor(dark, accent, wall) }
  if (!dark) {
    return {
      ...shared,
      '--wall-text': n(0.3, 2),
      '--grain-opacity': '0.55',
      '--glass': `oklch(0.99 0.004 ${nh} / .84)`,
      '--edge': `oklch(0.3 ${NEUTRAL_CHROMA} ${nh} / .12)`,
      '--shadow': `0 1px 2px oklch(0.3 0.03 ${nh} / .08), 0 18px 44px -10px oklch(0.3 0.03 ${nh} / .3)`,
      '--shadow-sm': `0 4px 14px -6px oklch(0.3 0.03 ${nh} / .25)`,
      '--shadow-menu': `0 1px 2px oklch(0.3 0.03 ${nh} / .1), 0 12px 32px -8px oklch(0.3 0.03 ${nh} / .3)`,
      '--win': n(0.995, 0.3),
      '--side': n(0.972, 0.6),
      '--text': n(0.24, 1.2),
      '--muted': n(0.47, 1.2),
      '--line': n(0.915, 0.7),
      '--line-2': n(0.955, 0.5),
      '--field': n(0.955, 0.6),
      '--track': n(0.88, 0.8),
      '--hover': n(0.955, 0.7),
      '--acc': a(0.52, ac),
      '--on-acc': '#fff',
      '--acc-soft': a(0.95, ac * 0.25),
      '--sel': a(0.925, ac * 0.32),
      '--ok': 'oklch(0.52 0.13 150)',
      '--ok-soft': 'oklch(0.95 0.04 150)',
      '--warn': 'oklch(0.62 0.14 70)',
      '--danger': 'oklch(0.52 0.18 25)',
      '--danger-soft': 'oklch(0.95 0.03 25)',
      '--syn-str': 'oklch(0.5 0.12 150)',
      '--syn-num': 'oklch(0.55 0.14 50)',
      '--scrim': `oklch(0.2 0.02 ${nh} / .28)`,
    }
  }
  return {
    ...shared,
    '--wall-text': n(0.86, 1.5),
    '--grain-opacity': '0.35',
    '--glass': `oklch(0.22 ${NEUTRAL_CHROMA} ${nh} / .86)`,
    '--edge': 'oklch(1 0 0 / .09)',
    '--shadow': '0 1px 2px rgba(0,0,0,.3), 0 22px 54px -12px rgba(0,0,0,.65)',
    '--shadow-sm': '0 6px 18px -8px rgba(0,0,0,.55)',
    '--shadow-menu': '0 14px 36px -8px rgba(0,0,0,.65)',
    '--win': n(0.215, 0.8),
    '--side': n(0.195, 0.8),
    '--text': n(0.94, 0.6),
    '--muted': n(0.72, 1),
    '--line': n(0.3, 0.8),
    '--line-2': n(0.25, 0.8),
    '--field': n(0.26, 0.8),
    '--track': n(0.35, 0.8),
    '--hover': n(0.26, 0.9),
    '--acc': a(0.76, ac * 0.85),
    '--on-acc': a(0.2, ac * 0.3),
    '--acc-soft': a(0.3, ac * 0.35),
    '--sel': a(0.33, ac * 0.42),
    '--ok': 'oklch(0.78 0.13 150)',
    '--ok-soft': 'oklch(0.3 0.05 150)',
    '--warn': 'oklch(0.8 0.13 75)',
    '--danger': 'oklch(0.74 0.15 25)',
    '--danger-soft': 'oklch(0.3 0.06 25)',
    '--syn-str': 'oklch(0.8 0.12 150)',
    '--syn-num': 'oklch(0.8 0.12 60)',
    '--scrim': 'rgba(0,0,0,.45)',
  }
}
