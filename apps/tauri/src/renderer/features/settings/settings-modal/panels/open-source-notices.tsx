import type { FileTermDesktopApi } from '@fileterm/core'
import type { MouseEvent } from 'react'
import ReactMarkdown from 'react-markdown'
import figNotices from '../../../../../../../../THIRD-PARTY-NOTICES.md?raw'
import bundledNotices from '../../../../../../../../THIRD_PARTY_NOTICES.md?raw'
import themeNotices from '../../../../../../../../docs/quality/iterm2-color-scheme-attribution.md?raw'
import { useExternalUrl } from '../../../../hooks/use-external-url'
import type { LocaleMessages } from '../../../../i18n'

const projectGroups = {
  openSourceInterface: {
    Tauri: 'https://tauri.app/',
    React: 'https://react.dev/',
    'xterm.js': 'https://xtermjs.org/',
    'Monaco Editor': 'https://microsoft.github.io/monaco-editor/'
  },
  openSourceTransfers: {
    russh: 'https://github.com/warp-tech/russh',
    'russh-sftp': 'https://github.com/AspectUnk/russh-sftp',
    suppaftp: 'https://github.com/veeso/suppaftp',
    keepawake: 'https://github.com/segevfiner/keepawake-rs'
  },
  openSourceAssets: {
    'Fig Autocomplete Specs': 'https://github.com/withfig/autocomplete',
    'Material Symbols': 'https://github.com/google/material-design-icons',
    Codicons: 'https://github.com/microsoft/vscode-codicons',
    'iTerm2-Color-Schemes': 'https://github.com/mbadolato/iTerm2-Color-Schemes'
  }
} satisfies Partial<Record<keyof LocaleMessages, Record<string, string>>>

const notices = [bundledNotices, figNotices, '## iTerm2-Color-Schemes\n\n' + themeNotices].join('\n\n')

export function OpenSourceNotices({
  desktopApi,
  t
}: {
  desktopApi: FileTermDesktopApi | undefined
  t: LocaleMessages
}) {
  const { openExternalUrl, externalUrlError } = useExternalUrl(desktopApi, t.openExternalUrlFailed)
  const openLink = (event: MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault()
    void openExternalUrl(event.currentTarget.href)
  }

  return (
    <div className="about-project-card">
      <dl className="open-source-projects">
        {Object.entries(projectGroups).map(([label, projects]) => (
          <div key={label}>
            <dt>{t[label as keyof typeof projectGroups]}</dt>
            <dd>
              {Object.entries(projects).map(([name, url]) => (
                <a href={url} key={name} onClick={openLink}>
                  {name}
                </a>
              ))}
            </dd>
          </div>
        ))}
      </dl>
      <details className="third-party-license">
        <summary>{t.thirdPartyLicense}</summary>
        <div className="open-source-license-text">
          <ReactMarkdown
            components={{
              a: ({ href, children }) =>
                href?.startsWith('https://') ? (
                  <a href={href} onClick={openLink}>
                    {children}
                  </a>
                ) : (
                  <>{children}</>
                ),
              h1: () => null,
              h2: ({ children }) => <h4>{children}</h4>,
              h3: ({ children }) => <h5>{children}</h5>
            }}
          >
            {notices}
          </ReactMarkdown>
        </div>
      </details>
      {externalUrlError ? (
        <p className="modal-error" role="alert">
          {externalUrlError}
        </p>
      ) : null}
    </div>
  )
}
