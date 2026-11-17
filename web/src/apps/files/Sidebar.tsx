// The left column of the file browser: home, root, favourites and disks.
import { Folder, HardDrive, House, Server, Star, Trash2 } from 'lucide-react'
import { memo } from 'react'
import type { Places } from '../../api/generated/Places'
import { t } from '../../i18n'
import { TRASH } from './changes'
import { baseName, formatSize } from './paths'

interface Props {
  path: string
  places: Places | null
  favourites: string[]
  onGo: (path: string) => void
}

export const Sidebar = memo(function Sidebar({ path, places, favourites, onGo }: Props) {
  const f = t.files
  const item = (target: string, label: string, icon: React.ReactNode, key = target) => (
    <button
      key={key}
      type="button"
      className="side-item"
      aria-current={path === target ? 'page' : undefined}
      title={target.startsWith(':') ? undefined : target}
      onClick={() => onGo(target)}
    >
      {icon}
      <span className="side-label">{label}</span>
    </button>
  )
  return (
    <nav className="files-side" aria-label={f.sidebar}>
      <div className="files-group">
        <h3>{f.groupPlaces}</h3>
        {places?.home && item(places.home, f.home, <House size={15} aria-hidden="true" />)}
        {item('/', f.root, <Server size={15} aria-hidden="true" />)}
        {item(TRASH, f.trash, <Trash2 size={15} aria-hidden="true" />)}
      </div>
      {favourites.length > 0 && (
        <div className="files-group">
          <h3>{f.groupFavourites}</h3>
          {favourites.map((p) => item(p, baseName(p), <Star size={15} aria-hidden="true" />, `fav:${p}`))}
        </div>
      )}
      {places && places.disks.length > 0 && (
        <div className="files-group">
          <h3>{f.groupDisks}</h3>
          {places.disks.map((disk) => {
            const used = disk.total - disk.free
            const percent = Math.round((used / disk.total) * 100)
            const use = f.diskUse(formatSize(used), formatSize(disk.total))
            return (
              <div key={`disk:${disk.path}`} className="files-disk">
                {item(
                  disk.path,
                  disk.path === '/' ? f.root : baseName(disk.path),
                  disk.path === '/' ? <HardDrive size={15} aria-hidden="true" /> : <Folder size={15} aria-hidden="true" />,
                  `disk:${disk.path}`,
                )}
                <div
                  className="files-disk-bar"
                  role="meter"
                  aria-label={use}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={percent}
                  data-level={percent >= 90 ? 'danger' : percent >= 80 ? 'warn' : 'ok'}
                >
                  <span style={{ width: `${percent}%` }} />
                </div>
                <span className="files-disk-text">{use}</span>
              </div>
            )
          })}
        </div>
      )}
    </nav>
  )
})
