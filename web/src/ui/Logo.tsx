// The RackDesk logo: a grid of 3 x 3 dots, 1 of them a green status light.
// The files it is drawn from are in assets/logo/.
import { t } from '../i18n'

/** The green dot. It keeps this colour in both themes. */
const LED = '#5edb81'
const TILE = '#5a66c7'

/** Centres of the dots, on a 64 x 64 grid. */
const AT = [20, 32, 44]
/** Row and column of the 2 dim dots and of the green dot. */
const DIM = ['1,1', '2,2']
const LIGHT = '0,2'

function Dots({ color }: { color: string }) {
  return (
    <>
      {AT.flatMap((cy, row) =>
        AT.map((cx, col) => {
          const key = `${row},${col}`
          return <circle key={key} cx={cx} cy={cy} r={5} fill={key === LIGHT ? LED : color} fillOpacity={DIM.includes(key) ? 0.4 : undefined} />
        }),
      )}
    </>
  )
}

/** The dots alone, in the current text colour. For use on a coloured
 * button, where the button gives the background. */
export function LogoMark({ size }: { size: number }) {
  return (
    <svg viewBox="12 12 40 40" width={size} height={size} aria-hidden="true">
      <Dots color="currentColor" />
    </svg>
  )
}

/** The app icon: white dots on the periwinkle tile. */
export function LogoTile({ size }: { size: number }) {
  return (
    <svg viewBox="0 0 64 64" width={size} height={size} aria-hidden="true">
      <rect width={64} height={64} rx={15} fill={TILE} />
      <Dots color="#ffffff" />
    </svg>
  )
}

/** The tile with the name next to it. */
export function Lockup({ size }: { size: number }) {
  return (
    <span className="lockup" style={{ fontSize: size * 0.62 }}>
      <LogoTile size={size} />
      <span>
        <b>{t.productRack}</b>
        {t.productDesk}
      </span>
    </span>
  )
}
