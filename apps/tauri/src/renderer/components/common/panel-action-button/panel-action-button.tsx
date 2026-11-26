import { Button, type ButtonProps } from '../button/button'
import './panel-action-button.css'

export type PanelActionButtonProps = Omit<ButtonProps, 'size' | 'variant'>

export function PanelActionButton({ className = '', ...rest }: PanelActionButtonProps) {
  const classes = ['ft-panel-action-btn', className].filter(Boolean).join(' ')
  return <Button {...rest} className={classes} size="panel" variant="ghost" />
}
