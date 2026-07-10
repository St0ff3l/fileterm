import type { ButtonHTMLAttributes } from 'react'
import { t } from '../../i18n'
import { AppIcon } from './AppIcon'

type CloseButtonSize = 'compact' | 'default' | 'tab' | 'window'

export function CloseButton({
  'aria-label': ariaLabel = t.closeTab,
  className,
  size = 'default',
  title,
  ...buttonProps
}: Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children' | 'type'> & {
  size?: CloseButtonSize
}) {
  const resolvedClassName = ['app-close-button', `app-close-button--${size}`, className].filter(Boolean).join(' ')

  return (
    <button
      {...buttonProps}
      aria-label={ariaLabel}
      className={resolvedClassName}
      title={title ?? ariaLabel}
      type="button"
    >
      <AppIcon name="close" size={size === 'default' ? 16 : size === 'compact' ? 14 : 12} />
    </button>
  )
}
