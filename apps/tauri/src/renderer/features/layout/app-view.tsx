import type { AppViewModel } from './app-view-model'
import { AppMainWorkspace } from './app-main-workspace'
import { AppModalPortals } from './app-modal-portals'
import { AppStandaloneWindows } from './app-standalone-windows'
import { FeedbackBubbleViewport } from '../../components/common/feedback-bubble/feedback-bubble'

export function AppView({ model }: { model: AppViewModel }) {
  return (
    <>
      {model.route.isMainWorkspaceWindow ? (
        <>
          <AppMainWorkspace model={model} />
          <AppModalPortals model={model} />
        </>
      ) : (
        <AppStandaloneWindows model={model} />
      )}
      <FeedbackBubbleViewport />
    </>
  )
}
