import { formatMessage, t } from '../../i18n'

export type SecurityOperation = 'load' | null

export type SecurityActionFeedback = {
  target: 'session' | 'backup'
  kind: 'success' | 'error'
  message: string
}

export const DEFAULT_IDLE_LOCK_MINUTES = 0
const IDLE_LOCK_PRESET_MINUTES = [1, 2, 3, 5, 10, 20, 30, 60, 90, 120, 150, 180, 0] as const

function formatIdleLockOption(minutes: number) {
  if (minutes === 0) {
    return t.securityIdleLockNever
  }

  const hours = Math.floor(minutes / 60)
  const remainingMinutes = minutes % 60
  if (hours === 0) {
    return formatMessage(t.securityIdleLockUnitTemplate, {
      unit: minutes === 1 ? t.securityMinute : t.securityMinutes,
      value: minutes
    })
  }

  const hourLabel = formatMessage(t.securityIdleLockUnitTemplate, {
    unit: hours === 1 ? t.securityHour : t.securityHours,
    value: hours
  })
  if (remainingMinutes === 0) {
    return hourLabel
  }

  const minuteLabel = formatMessage(t.securityIdleLockUnitTemplate, {
    unit: remainingMinutes === 1 ? t.securityMinute : t.securityMinutes,
    value: remainingMinutes
  })
  return formatMessage(t.securityIdleLockCompoundTemplate, {
    hours: hourLabel,
    minutes: minuteLabel
  })
}

export function getIdleLockOptions() {
  return IDLE_LOCK_PRESET_MINUTES.map((minutes) => ({
    label: formatIdleLockOption(minutes),
    value: String(minutes)
  }))
}

export function normalizeIdleLockMinutes(value: number) {
  return IDLE_LOCK_PRESET_MINUTES.includes(value as (typeof IDLE_LOCK_PRESET_MINUTES)[number])
    ? value
    : DEFAULT_IDLE_LOCK_MINUTES
}

export function localizeSecurityError(cause: unknown) {
  const message = cause instanceof Error ? cause.message : String(cause)
  const code = message.replace(/^command error:\s*/, '')
  if (code === 'SECURITY_LOCK_PASSWORD_REQUIRED') return t.securityLockPasswordRequired
  if (code === 'SECURITY_CURRENT_LOCK_PASSWORD_REQUIRED') return t.securityCurrentPasswordRequired
  if (code === 'SECURITY_CURRENT_LOCK_PASSWORD_INVALID') return t.securityCurrentPasswordInvalid
  if (code === 'SECURITY_CURRENT_BACKUP_PASSWORD_REQUIRED') return t.securityCurrentBackupPasswordRequired
  if (code === 'SECURITY_CURRENT_BACKUP_PASSWORD_INVALID') return t.securityCurrentBackupPasswordInvalid
  if (code === 'SECURITY_IDLE_LOCK_MINUTES_INVALID') return t.securityIdleLockMinutesInvalid
  return message
}
