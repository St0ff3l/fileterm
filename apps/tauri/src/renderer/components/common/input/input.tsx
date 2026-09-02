import type { InputHTMLAttributes, ReactNode, Ref, TextareaHTMLAttributes } from 'react'
import './input.css'

export type InputSize = 'sm' | 'md' | 'lg'

export type InputProps = Omit<InputHTMLAttributes<HTMLInputElement>, 'size'> & {
  ref?: Ref<HTMLInputElement>
  sizeVariant?: InputSize
  isError?: boolean
  prefix?: ReactNode
  suffix?: ReactNode
}

export function Input({
  ref,
  sizeVariant = 'md',
  isError = false,
  prefix,
  suffix,
  className = '',
  disabled = false,
  ...rest
}: InputProps) {
  const wrapperClasses = [
    'ft-input-wrapper',
    isError ? 'ft-input-wrapper--error' : '',
    disabled ? 'ft-input-wrapper--disabled' : '',
    className
  ]
    .filter(Boolean)
    .join(' ')

  const inputClasses = ['ft-input', `ft-input--${sizeVariant}`].filter(Boolean).join(' ')

  return (
    <div className={wrapperClasses}>
      {prefix && <span className="ft-input__prefix">{prefix}</span>}
      <input ref={ref} className={inputClasses} disabled={disabled} {...rest} />
      {suffix && <span className="ft-input__suffix">{suffix}</span>}
    </div>
  )
}

export type TextareaProps = TextareaHTMLAttributes<HTMLTextAreaElement> & {
  ref?: Ref<HTMLTextAreaElement>
  isError?: boolean
}

export function Textarea({ ref, isError = false, className = '', disabled = false, ...rest }: TextareaProps) {
  const classes = [
    'ft-textarea',
    isError ? 'ft-textarea--error' : '',
    disabled ? 'ft-textarea--disabled' : '',
    className
  ]
    .filter(Boolean)
    .join(' ')

  return <textarea ref={ref} className={classes} disabled={disabled} {...rest} />
}
