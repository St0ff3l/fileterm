import { useCallback } from 'react'
import { t } from '../../../../i18n'
import type { SettingsModalState } from './state'

export function useSettingsSecurityController({ state }: { state: SettingsModalState }) {
  const { setActiveTab, setSecurityNotice, setSecurityFocusRequest } = state

  const openSecuritySettings = (focusBackupPassword = false) => {
    setSecurityNotice(focusBackupPassword ? t.securityBackupPasswordRequired : null)
    if (focusBackupPassword) {
      setSecurityFocusRequest((current) => current + 1)
    }
    setActiveTab('security')
  }

  const handleSecurityBackupPasswordFocusHandled = useCallback(() => {
    setSecurityFocusRequest(0)
  }, [setSecurityFocusRequest])

  return {
    securityNotice: state.securityNotice,
    setSecurityNotice,
    securityFocusRequest: state.securityFocusRequest,
    openSecuritySettings,
    handleSecurityBackupPasswordFocusHandled
  }
}
