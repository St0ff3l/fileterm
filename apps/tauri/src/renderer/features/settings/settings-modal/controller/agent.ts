import { useEffect, useMemo } from 'react'
import { DEFAULT_MCP_AGENT_PREFERENCES, type McpAgentClientStatus, type McpAgentPreferences } from '@fileterm/core'
import { t } from '../../../../i18n'
import type { SettingsModalState } from './state'

export function useAgentSettingsController({
  state,
  onLaunchLocalAgent
}: {
  state: SettingsModalState
  onLaunchLocalAgent?: (client: McpAgentClientStatus) => void
}) {
  const {
    activeTab,
    desktopApi,
    mcpAgentPreferences,
    setMcpAgentPreferences,
    mcpAgentSetup,
    setMcpAgentSetup,
    mcpAgentProfiles,
    setMcpAgentProfiles,
    mcpAgentProfileSearch,
    setMcpAgentProfileSearch,
    mcpAgentOperation,
    setMcpAgentOperation,
    mcpAgentMessage,
    setMcpAgentMessage,
    agentSubTab,
    setAgentSubTab
  } = state

  const filteredMcpAgentProfiles = useMemo(() => {
    const query = mcpAgentProfileSearch.trim().toLocaleLowerCase()
    if (!query) return mcpAgentProfiles
    return mcpAgentProfiles.filter((profile) =>
      `${profile.name} ${profile.host} ${profile.type} ${profile.port}`.toLocaleLowerCase().includes(query)
    )
  }, [mcpAgentProfileSearch, mcpAgentProfiles])

  const selectedMcpAgentProfileCount = useMemo(
    () => mcpAgentProfiles.filter((profile) => mcpAgentPreferences.allowedProfileIds.includes(profile.id)).length,
    [mcpAgentPreferences.allowedProfileIds, mcpAgentProfiles]
  )

  const mcpExecutionPolicyOptions = [
    {
      value: 'read-only' as const,
      label: t.agentMcpExecutionReadOnly,
      description: t.agentMcpExecutionReadOnlyDescription
    },
    {
      value: 'basic-safe-operations' as const,
      label: t.agentMcpExecutionBasicSafe,
      description: t.agentMcpExecutionBasicSafeDescription
    },
    {
      value: 'full-access' as const,
      label: t.agentMcpExecutionFull,
      description: t.agentMcpExecutionFullDescription
    }
  ]

  const mcpConnectionScopeOptions = [
    {
      value: 'all-saved-connections' as const,
      label: t.agentMcpConnectionModeAll,
      description: t.agentMcpConnectionModeAllHint
    },
    {
      value: 'selected-connections' as const,
      label: t.agentMcpConnectionModeSelected,
      description: t.agentMcpConnectionModeSelectedHint
    }
  ]

  const mcpCapabilityRows = [
    { label: t.agentMcpCapabilityQuery, readOnly: true, basicSafe: true, full: true },
    { label: t.agentMcpCapabilityRemoteCommands, readOnly: false, basicSafe: true, full: true },
    { label: t.agentMcpCapabilityRemoteChanges, readOnly: false, basicSafe: true, full: true },
    { label: t.agentMcpCapabilityTunnels, readOnly: false, basicSafe: true, full: true },
    { label: t.agentMcpCapabilityDangerousCommands, readOnly: false, basicSafe: false, full: true },
    { label: t.agentMcpCapabilitySkipApproval, readOnly: false, basicSafe: false, full: true }
  ]

  const saveMcpAgentPreferences = (patch: Partial<McpAgentPreferences>) => {
    if (!desktopApi || mcpAgentOperation === 'save') {
      return
    }

    const previousPreferences = mcpAgentPreferences
    const nextPreferences = { ...mcpAgentPreferences, ...patch }
    if (
      nextPreferences.connectionScope === previousPreferences.connectionScope &&
      nextPreferences.operationPolicy === previousPreferences.operationPolicy &&
      nextPreferences.allowedProfileIds.length === previousPreferences.allowedProfileIds.length &&
      nextPreferences.allowedProfileIds.every(
        (profileId, index) => profileId === previousPreferences.allowedProfileIds[index]
      )
    ) {
      return
    }
    setMcpAgentPreferences(nextPreferences)
    setMcpAgentMessage(null)
    setMcpAgentOperation('save')
    void desktopApi
      .setUiPreferences({ mcpAgent: nextPreferences })
      .then((preferences) => {
        setMcpAgentPreferences({ ...DEFAULT_MCP_AGENT_PREFERENCES, ...preferences.mcpAgent })
        setMcpAgentMessage(t.agentMcpSaved)
      })
      .catch((error: unknown) => {
        setMcpAgentPreferences(previousPreferences)
        setMcpAgentMessage(error instanceof Error ? error.message : String(error))
      })
      .finally(() => setMcpAgentOperation(null))
  }

  const copyMcpAgentCommand = (command: string, successMessage: string) => {
    if (!desktopApi || !command) return
    setMcpAgentMessage(null)
    void desktopApi
      .writeClipboardText(command)
      .then(() => setMcpAgentMessage(successMessage))
      .catch((error: unknown) => setMcpAgentMessage(error instanceof Error ? error.message : String(error)))
  }

  const copyMcpAgentRegistrationCommand = (command: string) => {
    copyMcpAgentCommand(command, t.agentMcpCommandCopied)
  }

  const launchMcpAgentInLocalTerminal = (client: McpAgentClientStatus) => {
    if (!client.available || !onLaunchLocalAgent) return
    setMcpAgentMessage(null)
    onLaunchLocalAgent(client)
  }

  useEffect(() => {
    if (activeTab !== 'agent') return
    if (!desktopApi) {
      setMcpAgentMessage(t.agentMcpDesktopOnly)
      return
    }
    let canceled = false
    setMcpAgentOperation('load')
    setMcpAgentMessage(null)
    void Promise.all([desktopApi.getMcpAgentSetup(), desktopApi.getConnectionLibrary(), desktopApi.getUiPreferences()])
      .then(([setup, library, preferences]) => {
        if (canceled) return
        setMcpAgentSetup(setup)
        setMcpAgentProfiles(library.profiles)
        setMcpAgentPreferences({ ...DEFAULT_MCP_AGENT_PREFERENCES, ...preferences.mcpAgent })
      })
      .catch((error: unknown) => {
        if (!canceled) {
          setMcpAgentMessage(error instanceof Error ? error.message : String(error))
        }
      })
      .finally(() => {
        if (!canceled) setMcpAgentOperation(null)
      })
    return () => {
      canceled = true
    }
  }, [activeTab, desktopApi])

  return {
    mcpAgentPreferences,
    mcpAgentOperation,
    mcpExecutionPolicyOptions,
    mcpConnectionScopeOptions,
    mcpCapabilityRows,
    selectedMcpAgentProfileCount,
    mcpAgentProfiles,
    mcpAgentProfileSearch,
    setMcpAgentProfileSearch,
    filteredMcpAgentProfiles,
    saveMcpAgentPreferences,
    agentSubTab,
    setAgentSubTab,
    mcpAgentSetup,
    copyMcpAgentCommand,
    copyMcpAgentRegistrationCommand,
    launchMcpAgentInLocalTerminal,
    onLaunchLocalAgent,
    mcpAgentMessage
  }
}
