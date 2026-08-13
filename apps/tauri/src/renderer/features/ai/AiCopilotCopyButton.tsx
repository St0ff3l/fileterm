import { useEffect, useRef, useState } from 'react'
import { t } from '../../i18n'
import { AppIcon } from '../common/AppIcon'

export async function copyAiText(text: string) {
  if (window.fileterm?.writeClipboardText) {
    await window.fileterm.writeClipboardText(text)
    return
  }

  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text)
    return
  }

  const textarea = document.createElement('textarea')
  textarea.value = text
  textarea.setAttribute('readonly', '')
  textarea.style.position = 'fixed'
  textarea.style.opacity = '0'
  document.body.appendChild(textarea)
  textarea.select()
  const copied = document.execCommand('copy')
  document.body.removeChild(textarea)
  if (!copied) {
    throw new Error('Clipboard copy failed')
  }
}

export function AiCopilotCopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  const copiedTimerRef = useRef<number | null>(null)

  useEffect(() => {
    return () => {
      if (copiedTimerRef.current !== null) {
        window.clearTimeout(copiedTimerRef.current)
      }
    }
  }, [])

  const handleCopy = async () => {
    try {
      await copyAiText(text)
      setCopied(true)
      if (copiedTimerRef.current !== null) {
        window.clearTimeout(copiedTimerRef.current)
      }
      copiedTimerRef.current = window.setTimeout(() => setCopied(false), 1400)
    } catch {
      setCopied(false)
    }
  }

  return (
    <button
      aria-label={copied ? t.aiCopilotCommandCopied : t.aiCopilotCopyCommand}
      className="ai-copilot-copy-button"
      title={copied ? t.aiCopilotCommandCopied : t.aiCopilotCopyCommand}
      type="button"
      onClick={() => void handleCopy()}
    >
      <AppIcon name={copied ? 'check' : 'copy'} size={16} />
    </button>
  )
}
