import { Component, type ErrorInfo, type ReactNode } from 'react'
import { Button } from '../../components/common/button/button'
import '../../components/common/dialog/form-dialog.css'
import { t } from '../../i18n'
import './error-boundary.css'
import { reportRendererError } from '../../lib/renderer-error-log'

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
    reportRendererError('react-boundary', error, errorInfo.componentStack)
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
      <div className="modal-backdrop error-boundary-backdrop">
        <div
          className="modal-card form-dialog error-boundary-dialog"
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="error-boundary-title"
          aria-describedby="error-boundary-description"
        >
          <div className="modal-header">
            <span id="error-boundary-title">{t.errorBoundaryTitle}</span>
          </div>
          <div className="form-dialog__scroll-region">
            <div className="form-dialog__body">
              <p id="error-boundary-description">{t.errorBoundaryDescription}</p>
              <pre>{error.message}</pre>
            </div>
          </div>
          <div className="form-actions">
            <Button variant="primary" onClick={this.handleReload} type="button">
              {t.errorBoundaryReload}
            </Button>
          </div>
        </div>
      </div>
    )
  }
}
