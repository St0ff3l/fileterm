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
          color: 'var(--text-main, #1a1a1a)',
          fontFamily: "'SF Pro Text', 'PingFang SC', 'Microsoft YaHei', 'Segoe UI', sans-serif",
          backdropFilter: 'blur(8px)',
          WebkitBackdropFilter: 'blur(8px)',
          overflow: 'auto'
        }}
      >
        <div
          style={{
            maxWidth: 480,
            padding: 24,
            background: 'var(--dialog-surface, var(--bg-card, #fff))',
            borderRadius: 14,
            border: '1px solid var(--dialog-border, rgba(255, 255, 255, 0.12))',
            boxShadow: 'var(--dialog-shadow, 0 22px 70px rgba(0, 0, 0, 0.45))'
          }}
        >
          <h1 style={{ fontSize: 18, fontWeight: 600, margin: '0 0 12px' }}>{t.errorBoundaryTitle}</h1>
          <p style={{ fontSize: 14, lineHeight: 1.6, margin: '0 0 16px', opacity: 0.8 }}>
            {t.errorBoundaryDescription}
          </p>
          <pre
            style={{
              fontSize: 12,
              padding: 12,
              margin: '0 0 16px',
              background: 'var(--bg-elevated, rgba(255, 255, 255, 0.08))',
              borderRadius: 8,
              overflow: 'auto',
              fontFamily: 'var(--font-mono, monospace)',
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
              border: '1px solid var(--button-primary-border, rgba(255, 255, 255, 0.12))',
              borderRadius: 8,
              background: 'var(--button-primary-bg, #1687e8)',
              color: '#fff',
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
