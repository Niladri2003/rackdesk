# RackDesk logo

The mark is a grid of 3 x 3 dots. 1 dot is green, like a status light on a server.

| Folder | Holds |
|---|---|
| `svg/` | Vector files: the mark, the icon tile, the favicon, the lockups |
| `png/` | App and browser icons from 16 to 1024 px, dark and single-colour variants, lockups |

## Colours
| Name | Value | Use |
|---|---|---|
| Periwinkle | `#5a66c7` | The tile, light |
| Ink | `#4d57b7` | The mark on white |
| Light mark | `#abb9fe` | The mark on dark |
| Dark tile | `#1b1e2e` | The tile, dark |
| LED green | `#5edb81` | The green dot. `#03a14a` on white |

The wordmark is set in Onest: "Rack" in weight 650 and "Desk" in weight 400.

## Rules
- Smallest size: 16 px.
- Clear space: the width of 1 dot on every side.

## Where it is used
- `web/public/`: `favicon.svg`, `icon-32.png` (fallback), `icon-180.png` (Apple touch icon), `icon-192.png`.
- `web/src/ui/Logo.tsx`: the mark and the tile as React components, drawn from the same 9 dots.
- The top of the root `README.md`: the lockup PNG files.

For a lockup, use the PNG files. The 2 lockup SVG files need the Onest font on the machine that shows them, and `lockup-light.svg` has the dot colours swapped.
