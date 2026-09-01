import type { ReactNode } from 'react'

/**
 * Keeps an action button's label footprint stable while an operation runs.
 * Icon-bearing buttons reserve the icon slot for their spinner; text-only
 * buttons omit that slot entirely so the label stays centered without a
 * leading blank area.
 */
export function StableButtonContent({
  busy = false,
  busyLabel,
  icon,
  label,
  reserveLabel
}: {
  busy?: boolean
  busyLabel?: ReactNode
  icon?: ReactNode
  label: ReactNode
  reserveLabel?: ReactNode
}) {
  const resolvedBusyLabel = busyLabel ?? label
  const hasIcon = icon !== undefined && icon !== null

  return (
    <>
      {hasIcon || busy ? (
        <span aria-hidden="true" className="stable-button-icon">
          {busy ? <span className="button-spinner" /> : icon}
        </span>
      ) : null}
      <StableButtonLabel busy={busy} busyLabel={resolvedBusyLabel} label={label} reserveLabel={reserveLabel} />
    </>
  )
}

export function StableButtonLabel({
  busy = false,
  busyLabel,
  label,
  reserveLabel
}: {
  busy?: boolean
  busyLabel?: ReactNode
  label: ReactNode
  reserveLabel?: ReactNode
}) {
  const resolvedBusyLabel = busyLabel ?? label

  return (
    <span className="stable-button-label">
      <span>{busy ? resolvedBusyLabel : label}</span>
      <span aria-hidden="true" className="stable-button-label-reserve">
        {busy ? label : resolvedBusyLabel}
      </span>
      {reserveLabel !== undefined ? (
        <span aria-hidden="true" className="stable-button-label-reserve">
          {reserveLabel}
        </span>
      ) : null}
    </span>
  )
}
