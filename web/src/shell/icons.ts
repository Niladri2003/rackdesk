// Icons and key names shared by shell components.
import { CircleAlert, CircleCheck, Info, TriangleAlert } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { NoticeKind } from './notify'

export const NOTICE_ICON: Record<NoticeKind, LucideIcon> = {
  info: Info,
  success: CircleCheck,
  warning: TriangleAlert,
  error: CircleAlert,
}

/** The modifier key for shortcuts, as shown to the user. */
export const MOD_KEY = /Mac|iPhone|iPad/.test(navigator.platform) ? 'Cmd+' : 'Ctrl+'
