import type { AppIconVariant, FileTermDesktopApi } from '@fileterm/core'
import { MAX_UI_ZOOM_PERCENT, MIN_UI_ZOOM_PERCENT, UI_ZOOM_PERCENT_STEP } from '@fileterm/core'
import { DropdownSelect } from '../../../common/dropdown-select'
import { RadioCardGroup } from '../../../common/radio-card-group'
import { SelectionControl } from '../../../common/selection-control'
import { type LocaleMessages } from '../../../../i18n'
import { useSettingsModalContext } from '../context'
import dockClassic from '../../../../assets/app-icons/dock-classic.png'
import dockMaterial from '../../../../assets/app-icons/dock-material.png'
import dockClean from '../../../../assets/app-icons/dock-clean.png'
import platformClassic from '../../../../../../src-tauri/icons/app/shared/classic-512.png'
import platformMaterial from '../../../../../../src-tauri/icons/app/shared/material-512.png'
import platformClean from '../../../../../../src-tauri/icons/app/shared/clean-512.png'

type InterfaceDisplaySettingsContext = {
  t: LocaleMessages
  desktopApi: FileTermDesktopApi | undefined
  compactTabBar: boolean
  isSavingCompactTabBar: boolean
  compactTabBarError: string | null
  setCompactTabBarPreference(nextValue: boolean): void
  filePanelRememberRatio: boolean
  isSavingFilePanelPreference: boolean
  filePanelPreferenceError: string | null
  setFilePanelRememberRatioPreference(nextValue: boolean): void
  rememberWindowSize: boolean
  isSavingWindowSizePreference: boolean
  windowSizePreferenceError: string | null
  setRememberWindowSizePreference(nextValue: boolean): void
  showMacosMenuBarIcon: boolean
  isSavingMacosMenuBarIcon: boolean
  macosMenuBarIconPreferenceError: string | null
  setMacosMenuBarIconPreference(nextValue: boolean): void
  appIcon: AppIconVariant
  isSavingAppIcon: boolean
  appIconPreferenceError: string | null
  setAppIconPreference(nextValue: AppIconVariant): void
  uiZoomLocked: boolean
  isSavingUiZoomPreference: boolean
  uiZoomPreferenceError: string | null
  setUiZoomLockPreference(nextValue: boolean): void
  uiZoomPercent: number
  isSavingUiZoomPercent: boolean
  uiZoomPercentError: string | null
  setUiZoomPercentPreference(nextValue: number): void
  maskConnectionAddresses: boolean
  isSavingMaskConnectionAddresses: boolean
  maskConnectionAddressesError: string | null
  setMaskConnectionAddressesPreference(nextValue: boolean): void
}

/** Interface layout and window preferences. */
export function InterfaceDisplaySettingsSections() {
  const {
    t,
    desktopApi,
    compactTabBar,
    isSavingCompactTabBar,
    compactTabBarError,
    setCompactTabBarPreference,
    filePanelRememberRatio,
    isSavingFilePanelPreference,
    filePanelPreferenceError,
    setFilePanelRememberRatioPreference,
    rememberWindowSize,
    isSavingWindowSizePreference,
    windowSizePreferenceError,
    setRememberWindowSizePreference,
    showMacosMenuBarIcon,
    isSavingMacosMenuBarIcon,
    macosMenuBarIconPreferenceError,
    setMacosMenuBarIconPreference,
    appIcon,
    isSavingAppIcon,
    appIconPreferenceError,
    setAppIconPreference,
    uiZoomLocked,
    isSavingUiZoomPreference,
    uiZoomPreferenceError,
    setUiZoomLockPreference,
    uiZoomPercent,
    isSavingUiZoomPercent,
    uiZoomPercentError,
    setUiZoomPercentPreference,
    maskConnectionAddresses,
    isSavingMaskConnectionAddresses,
    maskConnectionAddressesError,
    setMaskConnectionAddressesPreference
  } = useSettingsModalContext<InterfaceDisplaySettingsContext>()
  const appIconHint =
    desktopApi?.platform === 'win32'
      ? t.appIconHintWindows
      : desktopApi?.platform === 'linux'
        ? t.appIconHintLinux
        : t.appIconHintMac
  const appIconPreviews =
    desktopApi?.platform === 'darwin'
      ? { classic: dockClassic, material: dockMaterial, clean: dockClean }
      : { classic: platformClassic, material: platformMaterial, clean: platformClean }

  return (
    <>
      <section className="settings-section">
        <h3>{t.displayAndFilePanelSettings}</h3>
        <p className="settings-tools-hint">{t.displayAndFilePanelSettingsHint}</p>
        <div aria-busy={isSavingUiZoomPercent} className="theme-config-control" inert={isSavingUiZoomPercent}>
          <span className="theme-config-label">{t.uiZoomPercent}</span>
          <DropdownSelect
            ariaLabel={t.uiZoomPercent}
            className="theme-config-select"
            disabled={!desktopApi}
            onChange={(value) => setUiZoomPercentPreference(Number(value))}
            options={Array.from(
              { length: (MAX_UI_ZOOM_PERCENT - MIN_UI_ZOOM_PERCENT) / UI_ZOOM_PERCENT_STEP + 1 },
              (_, index) => {
                const percent = MIN_UI_ZOOM_PERCENT + index * UI_ZOOM_PERCENT_STEP
                return { value: String(percent), label: `${percent}%` }
              }
            )}
            value={String(uiZoomPercent)}
          />
        </div>
        {uiZoomPercentError ? <p className="modal-error">{uiZoomPercentError}</p> : null}
        <div className="overview-preference-list">
          <label
            aria-busy={isSavingUiZoomPreference}
            className="overview-preference-row"
            inert={isSavingUiZoomPreference}
          >
            <span className="overview-preference-copy">
              <strong>{t.lockUiZoom}</strong>
              <p>{t.lockUiZoomHint}</p>
            </span>
            <span className="command-toggle overview-preference-toggle">
              <SelectionControl
                checked={uiZoomLocked}
                disabled={!desktopApi}
                onChange={(event) => setUiZoomLockPreference(event.target.checked)}
                type="checkbox"
              />
            </span>
          </label>
          <label aria-busy={isSavingCompactTabBar} className="overview-preference-row" inert={isSavingCompactTabBar}>
            <span className="overview-preference-copy">
              <strong>{t.compactTabBar}</strong>
              <p>{t.compactTabBarHint}</p>
            </span>
            <span className="command-toggle overview-preference-toggle">
              <SelectionControl
                checked={compactTabBar}
                disabled={!desktopApi}
                onChange={(event) => setCompactTabBarPreference(event.target.checked)}
                type="checkbox"
              />
            </span>
          </label>
          <label
            aria-busy={isSavingFilePanelPreference}
            className="overview-preference-row"
            inert={isSavingFilePanelPreference}
          >
            <span className="overview-preference-copy">
              <strong>{t.rememberFilePanelRatio}</strong>
              <p>{t.rememberFilePanelRatioHint}</p>
            </span>
            <span className="command-toggle overview-preference-toggle">
              <SelectionControl
                checked={filePanelRememberRatio}
                disabled={!desktopApi}
                onChange={(event) => setFilePanelRememberRatioPreference(event.target.checked)}
                type="checkbox"
              />
            </span>
          </label>
          <label
            aria-busy={isSavingMaskConnectionAddresses}
            className="overview-preference-row"
            inert={isSavingMaskConnectionAddresses}
          >
            <span className="overview-preference-copy">
              <strong>{t.maskConnectionAddresses}</strong>
              <p>{t.maskConnectionAddressesHint}</p>
            </span>
            <span className="command-toggle overview-preference-toggle">
              <SelectionControl
                checked={maskConnectionAddresses}
                disabled={!desktopApi}
                onChange={(event) => setMaskConnectionAddressesPreference(event.target.checked)}
                type="checkbox"
              />
            </span>
          </label>
        </div>
        {compactTabBarError ? (
          <p className="modal-error" role="alert">
            {compactTabBarError}
          </p>
        ) : null}
        {uiZoomPreferenceError ? <p className="modal-error">{uiZoomPreferenceError}</p> : null}
        {filePanelPreferenceError ? <p className="modal-error">{filePanelPreferenceError}</p> : null}
        {maskConnectionAddressesError ? <p className="modal-error">{maskConnectionAddressesError}</p> : null}
      </section>

      <section className="settings-section">
        <h3>{t.windowAppSettings}</h3>
        {desktopApi && ['darwin', 'win32', 'linux'].includes(desktopApi.platform) ? (
          <div className="theme-config-subsection">
            <h4>{t.appIcon}</h4>
            <p className="settings-tools-hint">{appIconHint}</p>
            <RadioCardGroup
              ariaLabel={t.appIcon}
              className="app-icon-picker"
              disabled={isSavingAppIcon}
              name="app-icon-variant"
              onChange={(value) => setAppIconPreference(value)}
              options={[
                {
                  value: 'classic',
                  label: (
                    <>
                      <img alt="" src={appIconPreviews.classic} />
                      <span>{t.appIconClassic}</span>
                    </>
                  ),
                  description: t.appIconClassicHint
                },
                {
                  value: 'material',
                  label: (
                    <>
                      <img alt="" src={appIconPreviews.material} />
                      <span>{t.appIconMaterial}</span>
                    </>
                  ),
                  description: t.appIconMaterialHint
                },
                {
                  value: 'clean',
                  label: (
                    <>
                      <img alt="" src={appIconPreviews.clean} />
                      <span>{t.appIconClean}</span>
                    </>
                  ),
                  description: t.appIconCleanHint
                }
              ]}
              value={appIcon}
            />
            {appIconPreferenceError ? <p className="modal-error">{appIconPreferenceError}</p> : null}
          </div>
        ) : null}
        {desktopApi?.platform === 'darwin' ? (
          <div className="overview-preference-list">
            <label
              aria-busy={isSavingMacosMenuBarIcon}
              className="overview-preference-row"
              inert={isSavingMacosMenuBarIcon}
            >
              <span className="overview-preference-copy">
                <strong>{t.showMacosMenuBarIcon}</strong>
                <p>{t.showMacosMenuBarIconHint}</p>
              </span>
              <span className="command-toggle overview-preference-toggle">
                <SelectionControl
                  checked={showMacosMenuBarIcon}
                  disabled={!desktopApi}
                  onChange={(event) => setMacosMenuBarIconPreference(event.target.checked)}
                  type="checkbox"
                />
              </span>
            </label>
          </div>
        ) : null}
        {macosMenuBarIconPreferenceError ? <p className="modal-error">{macosMenuBarIconPreferenceError}</p> : null}
        <div className="overview-preference-list">
          <label
            aria-busy={isSavingWindowSizePreference}
            className="overview-preference-row"
            inert={isSavingWindowSizePreference}
          >
            <span className="overview-preference-copy">
              <strong>{t.rememberWindowSize}</strong>
              <p>{t.rememberWindowSizeHint}</p>
            </span>
            <span className="command-toggle overview-preference-toggle">
              <SelectionControl
                checked={rememberWindowSize}
                disabled={!desktopApi}
                onChange={(event) => setRememberWindowSizePreference(event.target.checked)}
                type="checkbox"
              />
            </span>
          </label>
        </div>
        {windowSizePreferenceError ? <p className="modal-error">{windowSizePreferenceError}</p> : null}
      </section>
    </>
  )
}
