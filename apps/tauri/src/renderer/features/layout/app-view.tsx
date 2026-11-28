import type { AppViewModel } from './app-view-model'
import { AppMainWorkspace } from './app-main-workspace'
import { AppModalPortals } from './app-modal-portals'
import { AppStandaloneWindows } from './app-standalone-windows'
import { WebWindowOverlayHost } from './web-window-overlay-host'
import { FeedbackBubbleViewport } from '../../components/common/feedback-bubble/feedback-bubble'

export function AppView({ model }: { model: AppViewModel }) {
  const showWebOverlayHost = Boolean(
    model.route.isMainWorkspaceWindow && model.shell.desktopApi && !model.shell.desktopApi.isDesktop
  )
  return (
    <>
      {model.route.isMainWorkspaceWindow ? (
        <>
          <AppMainWorkspace model={model} />
          {showWebOverlayHost ? <WebWindowOverlayHost model={model} /> : null}
          <AppModalPortals model={model} />
        </>
      ) : (
        <AppStandaloneWindows model={model} />
      )}
      <FeedbackBubbleViewport />
    </>
  )
}
