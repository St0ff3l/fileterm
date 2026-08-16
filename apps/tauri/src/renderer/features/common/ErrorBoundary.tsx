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
            padding: 24,
            background: 'var(--dialog-surface, var(--bg-card, #181818))',
            borderRadius: 14,
            border: '1px solid var(--dialog-border, var(--border-light, rgba(255, 255, 255, 0.12)))',
            boxShadow: 'var(--dialog-shadow, 0 22px 70px rgba(0, 0, 0, 0.45))',
            color: 'var(--dialog-title, var(--text-main, #e0e0e0))'
          }}
        >
          <h1
            style={{
              fontSize: 18,
              fontWeight: 600,
              margin: '0 0 12px',
              color: 'var(--dialog-title, var(--text-main, #ffffff))'
            }}
          >
            {t.errorBoundaryTitle}
          </h1>
          <p
            style={{
              fontSize: 14,
              lineHeight: 1.6,
              margin: '0 0 16px',
              color: 'var(--dialog-description, var(--text-muted, rgba(255, 255, 255, 0.7)))'
            }}
          >
            {t.errorBoundaryDescription}
          </p>
          <pre
            style={{
              fontSize: 12,
              padding: 12,
              margin: '0 0 16px',
              background: 'var(--bg-elevated, var(--surface-inset, rgba(255, 255, 255, 0.08)))',
              borderRadius: 8,
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
          <button
            type="button"
            onClick={this.handleReload}
            style={{
              padding: '8px 20px',
              fontSize: 14,
              fontWeight: 500,
              border: '1px solid var(--button-primary-border, transparent)',
              borderRadius: 8,
              background: 'var(--button-primary-bg, var(--primary, #0169cc))',
              color: 'var(--button-primary-text, #ffffff)',
              cursor: 'pointer'
            }}
          >
            {t.errorBoundaryReload}
          </button>
        </div>
      </div>
    )
  }
}
