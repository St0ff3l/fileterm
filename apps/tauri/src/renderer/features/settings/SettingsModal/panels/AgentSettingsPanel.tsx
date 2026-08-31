import type { Dispatch, SetStateAction } from 'react'
import type {
  ConnectionProfile,
  FileTermDesktopApi,
  McpAgentClientStatus,
  McpAgentPreferences,
  McpAgentSetup
} from '@fileterm/core'
import { AppIcon } from '../../../common/AppIcon'
import { RadioCardGroup } from '../../../common/RadioCardGroup'
import { SelectionControl } from '../../../common/SelectionControl'
import { formatMessage, type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'
import { agentProfileTarget, FILETERM_CLI_SKILL_URL } from '../constants'

type McpPolicyOption = {
  value: McpAgentPreferences['operationPolicy']
  label: string
  description: string
}

type McpConnectionScopeOption = {
  value: McpAgentPreferences['connectionScope']
  label: string
  description: string
}

type McpCapabilityRow = {
  label: string
  readOnly: boolean
  basicSafe: boolean
  full: boolean
}

type AgentSettingsPanelContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  mcpAgentPreferences: McpAgentPreferences
  mcpAgentOperation: 'load' | 'save' | null
  mcpExecutionPolicyOptions: McpPolicyOption[]
  mcpConnectionScopeOptions: McpConnectionScopeOption[]
  mcpCapabilityRows: McpCapabilityRow[]
  selectedMcpAgentProfileCount: number
  mcpAgentProfiles: ConnectionProfile[]
  mcpAgentProfileSearch: string
  setMcpAgentProfileSearch: Dispatch<SetStateAction<string>>
  filteredMcpAgentProfiles: ConnectionProfile[]
  saveMcpAgentPreferences(patch: Partial<McpAgentPreferences>): void
  agentSubTab: 'mcp' | 'cli'
  setAgentSubTab: Dispatch<SetStateAction<'mcp' | 'cli'>>
  mcpAgentSetup: McpAgentSetup | null
  copyMcpAgentCommand(command: string, successMessage: string): void
  copyMcpAgentRegistrationCommand(command: string): void
  launchMcpAgentInLocalTerminal(client: McpAgentClientStatus): void
  onLaunchLocalAgent?: (client: McpAgentClientStatus) => void
  mcpAgentMessage: string | null
}

export function AgentSettingsPanel() {
  const {
    t,
    desktopApi,
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
  } = useSettingsModalContext<AgentSettingsPanelContext>()

  return (
    <div className="settings-panel settings-agent-mcp-panel">
      <section className="settings-section">
        <h3>{t.agentMcpSettings}</h3>
        <p className="settings-tools-hint">{t.agentMcpDescription}</p>

        <div className="agent-mcp-policy-stack agent-mcp-shared-policy">
          <section className="agent-mcp-policy-card" aria-labelledby="agent-mcp-execution-policy-title">
            <div className="agent-mcp-policy-heading">
              <div>
                <h4 id="agent-mcp-execution-policy-title">{t.agentMcpExecutionPolicyTitle}</h4>
                <p>{t.agentMcpExecutionPolicyDescription}</p>
              </div>
            </div>
            <RadioCardGroup
              ariaLabel={t.agentMcpExecutionPolicyTitle}
              className="agent-mcp-policy-options"
              disabled={!desktopApi || mcpAgentOperation !== null}
              name="agent-mcp-execution-policy"
              options={mcpExecutionPolicyOptions}
              value={mcpAgentPreferences.operationPolicy}
              onChange={(value) => saveMcpAgentPreferences({ operationPolicy: value })}
            />
            <p
              className={`agent-mcp-policy-notice ${
                mcpAgentPreferences.operationPolicy === 'full-access' ? 'is-warning' : ''
              }`}
            >
              <AppIcon
                name={mcpAgentPreferences.operationPolicy === 'full-access' ? 'shield' : 'shield-check'}
                size={14}
                strokeWidth={2}
              />
              <span>
                {mcpAgentPreferences.operationPolicy === 'full-access'
                  ? t.agentMcpExecutionFullWarning
                  : t.agentMcpExecutionBoundary}
              </span>
            </p>
            <div className="agent-mcp-capability">
              <h5>{t.agentMcpCapabilityTitle}</h5>
              <div aria-label={t.agentMcpCapabilityTitle} className="agent-mcp-capability-table" role="table">
                <div className="agent-mcp-capability-row is-header" role="row">
                  <span role="columnheader">{t.agentMcpCapabilityHeader}</span>
                  <span role="columnheader">{t.agentMcpCapabilityReadOnly}</span>
                  <span role="columnheader">{t.agentMcpCapabilityBasicSafe}</span>
                  <span role="columnheader">{t.agentMcpCapabilityFull}</span>
                </div>
                {mcpCapabilityRows.map((row) => (
                  <div key={row.label} className="agent-mcp-capability-row" role="row">
                    <span role="cell">{row.label}</span>
                    {[row.readOnly, row.basicSafe, row.full].map((allowed, index) => (
                      <span
                        key={`${row.label}-${index}`}
                        aria-label={allowed ? t.agentMcpCapabilityAllowed : t.agentMcpCapabilityDenied}
                        className={`agent-mcp-capability-value ${allowed ? 'is-allowed' : 'is-denied'}`}
                        role="cell"
                      >
                        <AppIcon name={allowed ? 'check' : 'close'} size={13} strokeWidth={2.2} />
                      </span>
                    ))}
                  </div>
                ))}
              </div>
            </div>
          </section>

          <section className="agent-mcp-policy-card" aria-labelledby="agent-mcp-allowed-connections-title">
            <div className="agent-mcp-policy-heading agent-mcp-connections-heading">
              <div>
                <h4 id="agent-mcp-allowed-connections-title">{t.agentMcpAllowedConnectionsTitle}</h4>
                <p>{t.agentMcpAllowedConnectionsDescription}</p>
              </div>
              <span className="agent-mcp-policy-count">
                {mcpAgentPreferences.connectionScope === 'selected-connections'
                  ? formatMessage(t.agentMcpSelectedConnectionCount, {
                      count: selectedMcpAgentProfileCount,
                      total: mcpAgentProfiles.length
                    })
                  : t.agentMcpConnectionModeAllStatus}
              </span>
            </div>
            <RadioCardGroup
              ariaLabel={t.agentMcpAllowedConnectionsTitle}
              className="agent-mcp-policy-options agent-mcp-connection-options"
              disabled={!desktopApi || mcpAgentOperation !== null}
              name="agent-mcp-connection-scope"
              options={mcpConnectionScopeOptions}
              value={mcpAgentPreferences.connectionScope}
              onChange={(value) => saveMcpAgentPreferences({ connectionScope: value })}
            />

            {mcpAgentPreferences.connectionScope === 'selected-connections' ? (
              <div className="agent-mcp-selected-connections">
                <div className="agent-mcp-selected-connections-heading">
                  <span>{t.agentMcpSelectedConnections}</span>
                </div>
                <input
                  aria-label={t.agentMcpSelectedConnections}
                  className="agent-mcp-profile-search"
                  disabled={!desktopApi || mcpAgentOperation !== null}
                  placeholder={t.agentMcpSelectedConnectionsSearchPlaceholder}
                  type="search"
                  value={mcpAgentProfileSearch}
                  onChange={(event) => setMcpAgentProfileSearch(event.target.value)}
                />
                <div className="agent-mcp-profile-list" role="group">
                  {filteredMcpAgentProfiles.map((profile) => {
                    const selected = mcpAgentPreferences.allowedProfileIds.includes(profile.id)
                    return (
                      <label key={profile.id} className="agent-mcp-profile-option">
                        <SelectionControl
                          checked={selected}
                          disabled={!desktopApi || mcpAgentOperation !== null}
                          type="checkbox"
                          onChange={() => {
                            const allowedProfileIds = selected
                              ? mcpAgentPreferences.allowedProfileIds.filter((id) => id !== profile.id)
                              : [...mcpAgentPreferences.allowedProfileIds, profile.id]
                            saveMcpAgentPreferences({ allowedProfileIds })
                          }}
                        />
                        <span className="agent-mcp-profile-copy">
                          <strong>{profile.name || profile.type.toUpperCase()}</strong>
                          <small>
                            {profile.type.toUpperCase()} · {agentProfileTarget(profile)} ·{' '}
                            {profile.hasSavedPassword ? t.agentMcpCredentialSaved : t.agentMcpCredentialPrompt}
                          </small>
                        </span>
                      </label>
                    )
                  })}
                  {!filteredMcpAgentProfiles.length ? (
                    <small className="agent-mcp-profile-empty">{t.agentMcpSelectedConnectionsEmpty}</small>
                  ) : null}
                </div>
                {!selectedMcpAgentProfileCount ? (
                  <small className="agent-mcp-profile-warning">{t.agentMcpSelectedConnectionsNone}</small>
                ) : null}
              </div>
            ) : null}
          </section>
        </div>

        <div className="agent-mcp-subtabs" role="tablist" aria-label={t.agentMcpSubTabs}>
          <button
            id="agent-mcp-tab-mcp"
            aria-controls="agent-mcp-panel-mcp"
            aria-selected={agentSubTab === 'mcp'}
            className={`agent-mcp-subtab-button ${agentSubTab === 'mcp' ? 'active' : ''}`}
            role="tab"
            type="button"
            onClick={() => setAgentSubTab('mcp')}
          >
            {t.agentMcpTabMcp}
          </button>
          <button
            id="agent-mcp-tab-cli"
            aria-controls="agent-mcp-panel-cli"
            aria-selected={agentSubTab === 'cli'}
            className={`agent-mcp-subtab-button ${agentSubTab === 'cli' ? 'active' : ''}`}
            role="tab"
            type="button"
            onClick={() => setAgentSubTab('cli')}
          >
            {t.agentMcpTabCli}
          </button>
        </div>

        {agentSubTab === 'mcp' ? (
          <div
            id="agent-mcp-panel-mcp"
            className="agent-mcp-tabpanel"
            role="tabpanel"
            aria-labelledby="agent-mcp-tab-mcp"
          >
            <div className="agent-mcp-runtime-card">
              <span className="agent-mcp-runtime-icon">
                <AppIcon name="terminal-file" size={17} strokeWidth={2} />
              </span>
              <div>
                <strong>{t.agentMcpRuntimeTitle}</strong>
                <p>{t.agentMcpRuntimeDescription}</p>
              </div>
            </div>

            <div className="agent-mcp-clients" aria-busy={mcpAgentOperation === 'load'}>
              <h4>{t.agentMcpClients}</h4>
              {mcpAgentSetup?.clients.map((client) => (
                <article key={client.id} className="agent-mcp-client-card">
                  <div className="agent-mcp-client-heading">
                    <div>
                      <strong>{client.label}</strong>
                      <small>{client.available ? t.agentMcpClientAvailable : t.agentMcpClientUnavailable}</small>
                    </div>
                    <span className={`agent-mcp-client-status ${client.available ? 'is-available' : ''}`}>
                      {client.command}
                    </span>
                  </div>
                  <div className="agent-mcp-registration">
                    <code>{client.registrationCommand}</code>
                    <button
                      aria-label={t.agentMcpRegistration}
                      className="copy-icon-button agent-mcp-copy-button"
                      disabled={!desktopApi}
                      title={t.agentMcpRegistration}
                      type="button"
                      onClick={() => copyMcpAgentRegistrationCommand(client.registrationCommand)}
                    >
                      <AppIcon name="copy" size={14} strokeWidth={2} />
                    </button>
                  </div>
                  <div className="agent-mcp-client-actions">
                    <small className="agent-mcp-registration-hint">{t.agentMcpRegistrationDescription}</small>
                    <button
                      className="ai-settings-secondary-button agent-mcp-launch-button"
                      disabled={!client.available || !onLaunchLocalAgent}
                      title={client.available ? t.agentMcpLaunchDescription : t.agentMcpClientUnavailable}
                      type="button"
                      onClick={() => launchMcpAgentInLocalTerminal(client)}
                    >
                      <AppIcon name="terminal-file" size={14} strokeWidth={2} />
                      {t.agentMcpLaunch}
                    </button>
                  </div>
                </article>
              ))}
            </div>

            <div className="agent-mcp-keep-open">
              <AppIcon name="server" size={15} />
              <div>
                <strong>{t.agentMcpKeepOpenTitle}</strong>
                <p>{t.agentMcpKeepOpenDescription}</p>
              </div>
            </div>
            {mcpAgentMessage ? <p className="agent-mcp-operation-message">{mcpAgentMessage}</p> : null}
          </div>
        ) : (
          <div
            id="agent-mcp-panel-cli"
            className="agent-mcp-tabpanel"
            role="tabpanel"
            aria-labelledby="agent-mcp-tab-cli"
          >
            {mcpAgentSetup ? (
              <div className="agent-mcp-direct-cli-card">
                <div>
                  <strong>{t.agentMcpDirectCliTitle}</strong>
                  <p>{t.agentMcpDirectCliDescription}</p>
                </div>
                <div className="agent-mcp-direct-cli-commands">
                  <div className="agent-mcp-direct-cli-command agent-mcp-doc-reference">
                    <small>{t.agentMcpCliSkillPath}</small>
                    <div className="agent-mcp-registration">
                      <code>{FILETERM_CLI_SKILL_URL}</code>
                      <button
                        aria-label={t.agentMcpCliSkillCopy}
                        className="copy-icon-button agent-mcp-copy-button"
                        disabled={!desktopApi}
                        title={t.agentMcpCliSkillCopy}
                        type="button"
                        onClick={() => copyMcpAgentCommand(FILETERM_CLI_SKILL_URL, t.agentMcpCliSkillCopied)}
                      >
                        <AppIcon name="copy" size={14} strokeWidth={2} />
                      </button>
                    </div>
                  </div>
                  <div className="agent-mcp-direct-cli-command agent-mcp-persistent-agent-command">
                    <small>{t.agentMcpPersistentAgentPath}</small>
                    <div className="agent-mcp-registration">
                      <code>{mcpAgentSetup.filetermCommand} cli --jsonl</code>
                      <button
                        aria-label={t.agentMcpPersistentAgentCopy}
                        className="copy-icon-button agent-mcp-copy-button"
                        disabled={!desktopApi}
                        title={t.agentMcpPersistentAgentCopy}
                        type="button"
                        onClick={() =>
                          copyMcpAgentCommand(
                            `${mcpAgentSetup.filetermCommand} cli --jsonl`,
                            t.agentMcpPersistentAgentCopied
                          )
                        }
                      >
                        <AppIcon name="copy" size={14} strokeWidth={2} />
                      </button>
                    </div>
                  </div>
                  <div className="agent-mcp-direct-cli-command">
                    <small>{t.agentMcpDirectCliPath}</small>
                    <div className="agent-mcp-registration">
                      <code>{mcpAgentSetup.filetermCommand} cli --help</code>
                      <button
                        aria-label={t.agentMcpDirectCliCopy}
                        className="copy-icon-button agent-mcp-copy-button"
                        disabled={!desktopApi}
                        title={t.agentMcpDirectCliCopy}
                        type="button"
                        onClick={() =>
                          copyMcpAgentCommand(`${mcpAgentSetup.filetermCommand} cli --help`, t.agentMcpDirectCliCopied)
                        }
                      >
                        <AppIcon name="copy" size={14} strokeWidth={2} />
                      </button>
                    </div>
                  </div>
                </div>
                <div className="agent-mcp-keep-open">
                  <AppIcon name="server" size={15} />
                  <div>
                    <strong>{t.agentMcpProcessModelTitle}</strong>
                    <p>{t.agentMcpProcessModelDescription}</p>
                  </div>
                </div>
              </div>
            ) : null}
            {mcpAgentMessage ? <p className="agent-mcp-operation-message">{mcpAgentMessage}</p> : null}
          </div>
        )}
      </section>
    </div>
  )
}
