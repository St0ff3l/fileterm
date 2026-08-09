import type { MouseEvent } from 'react'
import ReactMarkdown, { type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'

const allowedElements = [
  'a',
  'blockquote',
  'br',
  'code',
  'del',
  'em',
  'h1',
  'h2',
  'h3',
  'h4',
  'h5',
  'h6',
  'hr',
  'li',
  'ol',
  'p',
  'pre',
  'strong',
  'table',
  'tbody',
  'td',
  'th',
  'thead',
  'tr',
  'ul'
]

function safeExternalHref(value: string) {
  try {
    const url = new URL(value)
    return url.protocol === 'http:' || url.protocol === 'https:' ? url.toString() : undefined
  } catch {
    return undefined
  }
}

function openExternalLink(event: MouseEvent<HTMLAnchorElement>, href: string) {
  event.preventDefault()
  void window.fileterm?.openExternalUrl(href)
}

const markdownComponents: Components = {
  a: ({ children, href }) => {
    const safeHref = href ? safeExternalHref(href) : undefined
    if (!safeHref) {
      return <span className="ai-copilot-markdown-invalid-link">{children}</span>
    }

    return (
      <a
        href={safeHref}
        onAuxClick={(event) => event.preventDefault()}
        onClick={(event) => openExternalLink(event, safeHref)}
      >
        {children}
        <span aria-hidden="true" className="material-symbols-outlined">
          open_in_new
        </span>
      </a>
    )
  }
}

export function AiCopilotMarkdown({ content }: { content: string }) {
  return (
    <div className="ai-copilot-message-markdown">
      <ReactMarkdown
        allowedElements={allowedElements}
        components={markdownComponents}
        remarkPlugins={[remarkGfm]}
        skipHtml
        urlTransform={(url) => safeExternalHref(url) ?? ''}
      >
        {content}
      </ReactMarkdown>
    </div>
  )
}
