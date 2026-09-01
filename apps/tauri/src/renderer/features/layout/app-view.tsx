import type { AppViewModel } from './app-view-model'
import { AppMainWorkspace } from './app-main-workspace'
import { AppModalPortals } from './app-modal-portals'
import { AppStandaloneWindows } from './app-standalone-windows'

export function AppView({ model }: { model: AppViewModel }) {
  if (!model.route.isMainWorkspaceWindow) {
    return <AppStandaloneWindows model={model} />
  }

  return (
    <>
      <AppMainWorkspace model={model} />
      <AppModalPortals model={model} />
    </>
  )
}
