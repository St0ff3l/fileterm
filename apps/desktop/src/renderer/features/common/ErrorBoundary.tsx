import { Component, type ErrorInfo, type ReactNode } from 'react'

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

    const isZhCN = navigator.language.toLowerCase().startsWith('zh')
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
          background: 'var(--bg-main, #f5f5f5)',
          color: 'var(--text-main, #1a1a1a)',
          fontFamily: "'SF Pro Text', 'PingFang SC', 'Microsoft YaHei', 'Segoe UI', sans-serif",
          overflow: 'auto'
        }}
      >
        <div
          style={{
            maxWidth: 480,
            padding: 32,
            background: 'var(--bg-card, #fff)',
            borderRadius: 12,
            border: '1px solid var(--border, rgba(0, 0, 0, 0.1))'
          }}
        >
          <h1 style={{ fontSize: 18, fontWeight: 600, margin: '0 0 12px' }}>
            {isZhCN ? '应用遇到错误' : 'The app encountered an error'}
          </h1>
          <p style={{ fontSize: 14, lineHeight: 1.6, margin: '0 0 16px', opacity: 0.8 }}>
            {isZhCN
              ? '发生了一个未处理的异常，请重新加载应用继续使用。'
              : 'An unhandled exception occurred. Please reload the app to continue.'}
          </p>
          <pre
            style={{
              fontSize: 12,
              padding: 12,
              margin: '0 0 16px',
              background: 'var(--bg-elevated, #f0f0f0)',
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
              border: 'none',
              borderRadius: 8,
              background: 'var(--accent-primary, #534ab7)',
              color: '#fff',
              cursor: 'pointer'
            }}
          >
            {isZhCN ? '重新加载' : 'Reload'}
          </button>
        </div>
      </div>
    )
  }
}
