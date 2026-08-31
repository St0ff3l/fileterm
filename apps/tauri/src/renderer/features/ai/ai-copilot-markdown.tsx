import { isValidElement, useRef, type MouseEvent, type ReactNode } from 'react'
import ReactMarkdown, { type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { t } from '../../i18n'
import { VerticalScrollbar } from '../common/vertical-scrollbar'
import { AiCopilotCopyButton } from './ai-copilot-copy-button'

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

function getCodeBlockText(children: ReactNode) {
  const child = Array.isArray(children) ? children.find(isValidElement) : isValidElement(children) ? children : null
  if (!child) {
    return String(children ?? '').replace(/\n$/, '')
  }

  const codeElement = child as typeof child & {
    props: { children?: ReactNode }
  }
  return String(codeElement.props.children ?? '').replace(/\n$/, '')
}

function MarkdownCodeBlock({ children }: { children?: ReactNode }) {
  const text = getCodeBlockText(children)
  const scrollRef = useRef<HTMLPreElement>(null)

  return (
    <div className="ai-copilot-markdown-code-block">
      <pre ref={scrollRef}>{children}</pre>
      <AiCopilotCopyButton text={text} />
      <VerticalScrollbar ariaLabel={t.scrollContent} scrollRef={scrollRef} />
    </div>
  )
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
  },
  pre: ({ children }) => <MarkdownCodeBlock>{children}</MarkdownCodeBlock>
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
