import { Component, type ErrorInfo, type ReactNode } from 'react'
import { t } from '../../i18n'

interface Props {
  children: ReactNode
}

interface State {
  error: Error | null
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('[FileTerm] Uncaught error:', error, errorInfo)
  }

  handleReload = () => {
    window.location.reload()
  }

  render() {
    const { error } = this.state
    if (!error) {
      return this.props.children
    }

    return (
      <div
        role="alert"
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          minHeight: '100vh',
          boxSizing: 'border-box',
          padding: 24,
          background: 'var(--modal-backdrop-bg, rgba(0, 0, 0, 0.78))',
          color: 'var(--text-main, #e0e0e0)',
          fontFamily: "'SF Pro Text', 'PingFang SC', 'Microsoft YaHei', 'Segoe UI', sans-serif",
          backdropFilter: 'blur(8px)',
          WebkitBackdropFilter: 'blur(8px)',
          overflow: 'auto'
        }}
      >
        <div
          style={{
            maxWidth: 480,
            width: '100%',
            padding: 0,
            background: 'var(--dialog-surface, var(--bg-card, #181818))',
            borderRadius: 'var(--radius-lg, 10px)',
            border: '1px solid var(--dialog-border, var(--border-light, rgba(255, 255, 255, 0.12)))',
            boxShadow: 'var(--dialog-shadow, 0 22px 70px rgba(0, 0, 0, 0.45))',
            color: 'var(--dialog-title, var(--text-main, #e0e0e0))',
            overflow: 'hidden'
          }}
        >
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              minHeight: 48,
              boxSizing: 'border-box',
              padding: '0 20px',
              borderBottom: '1px solid var(--dialog-footer-border, var(--dialog-border, rgba(255, 255, 255, 0.12)))',
              background: 'var(--bg-sidebar, var(--dialog-surface, #181818))'
            }}
          >
            <h1
              style={{
                fontSize: 18,
                fontWeight: 600,
                margin: 0,
                color: 'var(--dialog-title, var(--text-main, #ffffff))'
              }}
            >
              {t.errorBoundaryTitle}
            </h1>
          </div>
          <div
            style={{
              display: 'grid',
              gap: 12,
              padding: 20
            }}
          >
            <p
              style={{
                fontSize: 14,
                lineHeight: 1.6,
                margin: 0,
                color: 'var(--dialog-description, var(--text-muted, rgba(255, 255, 255, 0.7)))'
              }}
            >
              {t.errorBoundaryDescription}
            </p>
            <pre
              style={{
                fontSize: 12,
                padding: 12,
                margin: 0,
                background: 'var(--bg-elevated, var(--surface-inset, rgba(255, 255, 255, 0.08)))',
                borderRadius: 'var(--radius-sm, 4px)',
                overflow: 'auto',
                fontFamily: 'var(--font-mono, monospace)',
                color: 'var(--text-main, #e0e0e0)',
                border: '1px solid var(--border-light, rgba(255, 255, 255, 0.08))',
                whiteSpace: 'pre-wrap',
                wordBreak: 'break-word'
              }}
            >
              {error.message}
            </pre>
          </div>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'flex-end',
              minHeight: 58,
              boxSizing: 'border-box',
              padding: '10px 20px 12px',
              borderTop: '1px solid var(--dialog-footer-border, var(--dialog-border, rgba(255, 255, 255, 0.12)))',
              background: 'var(--dialog-surface, var(--bg-card, #181818))'
            }}
          >
            <button
              type="button"
              onClick={this.handleReload}
              style={{
                boxSizing: 'border-box',
                display: 'inline-flex',
                alignItems: 'center',
                justifyContent: 'center',
                height: 36,
                minHeight: 36,
                padding: '0 12px',
                fontFamily: 'inherit',
                fontSize: 14,
                fontWeight: 500,
                lineHeight: 1,
                border: '1px solid var(--button-primary-border, transparent)',
                borderRadius: 'var(--radius-md, 6px)',
                background: 'var(--button-primary-bg, var(--primary, #0169cc))',
                color: 'var(--button-primary-text, #ffffff)',
                cursor: 'pointer'
              }}
            >
              {t.errorBoundaryReload}
            </button>
          </div>
        </div>
      </div>
    )
  }
}
