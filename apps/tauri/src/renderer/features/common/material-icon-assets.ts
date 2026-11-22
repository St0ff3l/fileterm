import addIcon from '../../assets/icons/actions/material-add.svg?raw'
import addCommentIcon from '../../assets/icons/actions/material-add_comment.svg?raw'
import adminPanelSettingsIcon from '../../assets/icons/security/material-admin_panel_settings.svg?raw'
import appsIcon from '../../assets/icons/navigation/material-apps.svg?raw'
import arrowBackIcon from '../../assets/icons/navigation/material-arrow_back.svg?raw'
import arrowForwardIcon from '../../assets/icons/navigation/material-arrow_forward.svg?raw'
import autoAwesomeIcon from '../../assets/icons/system/material-auto_awesome.svg?raw'
import checkIcon from '../../assets/icons/actions/material-check.svg?raw'
import chevronRightIcon from '../../assets/icons/navigation/material-chevron_right.svg?raw'
import closeIcon from '../../assets/icons/navigation/material-close.svg?raw'
import cloudIcon from '../../assets/icons/network/material-cloud.svg?raw'
import cloudSyncIcon from '../../assets/icons/network/material-cloud_sync.svg?raw'
import dashboardIcon from '../../assets/icons/navigation/material-dashboard.svg?raw'
import deleteIcon from '../../assets/icons/actions/material-delete.svg?raw'
import deleteSweepIcon from '../../assets/icons/actions/material-delete_sweep.svg?raw'
import descriptionIcon from '../../assets/icons/files/material-description.svg?raw'
import dnsIcon from '../../assets/icons/network/material-dns.svg?raw'
import downloadIcon from '../../assets/icons/files/material-download.svg?raw'
import downloadingIcon from '../../assets/icons/files/material-downloading.svg?raw'
import dragIndicatorIcon from '../../assets/icons/actions/material-drag_indicator.svg?raw'
import editIcon from '../../assets/icons/actions/material-edit.svg?raw'
import editNoteIcon from '../../assets/icons/actions/material-edit_note.svg?raw'
import errorIcon from '../../assets/icons/system/material-error.svg?raw'
import fingerprintIcon from '../../assets/icons/security/material-fingerprint.svg?raw'
import fitScreenIcon from '../../assets/icons/actions/material-fit_screen.svg?raw'
import folderIcon from '../../assets/icons/files/material-folder.svg?raw'
import folderOpenIcon from '../../assets/icons/files/material-folder_open.svg?raw'
import folderSharedIcon from '../../assets/icons/files/material-folder_shared.svg?raw'
import forumIcon from '../../assets/icons/system/material-forum.svg?raw'
import historyIcon from '../../assets/icons/system/material-history.svg?raw'
import homeIcon from '../../assets/icons/navigation/material-home.svg?raw'
import infoIcon from '../../assets/icons/system/material-info.svg?raw'
import keyIcon from '../../assets/icons/security/material-key.svg?raw'
import keyOffIcon from '../../assets/icons/security/material-key_off.svg?raw'
import lockIcon from '../../assets/icons/security/material-lock.svg?raw'
import moreHorizIcon from '../../assets/icons/actions/material-more_horiz.svg?raw'
import openInNewIcon from '../../assets/icons/navigation/material-open_in_new.svg?raw'
import paletteIcon from '../../assets/icons/system/material-palette.svg?raw'
import previewIcon from '../../assets/icons/system/material-preview.svg?raw'
import progressActivityIcon from '../../assets/icons/system/material-progress_activity.svg?raw'
import refreshIcon from '../../assets/icons/actions/material-refresh.svg?raw'
import restartAltIcon from '../../assets/icons/actions/material-restart_alt.svg?raw'
import saveIcon from '../../assets/icons/actions/material-save.svg?raw'
import saveAsIcon from '../../assets/icons/actions/material-save_as.svg?raw'
import searchIcon from '../../assets/icons/actions/material-search.svg?raw'
import settingsIcon from '../../assets/icons/system/material-settings.svg?raw'
import settingsEthernetIcon from '../../assets/icons/network/material-settings_ethernet.svg?raw'
import settingsSuggestIcon from '../../assets/icons/system/material-settings_suggest.svg?raw'
import shieldIcon from '../../assets/icons/security/material-shield.svg?raw'
import starIcon from '../../assets/icons/system/material-star.svg?raw'
import stopIcon from '../../assets/icons/actions/material-stop.svg?raw'
import systemUpdateAltIcon from '../../assets/icons/system/material-system_update_alt.svg?raw'
import terminalIcon from '../../assets/icons/network/material-terminal.svg?raw'
import translateIcon from '../../assets/icons/system/material-translate.svg?raw'
import tuneIcon from '../../assets/icons/system/material-tune.svg?raw'
import uploadFileIcon from '../../assets/icons/files/material-upload_file.svg?raw'
import usbIcon from '../../assets/icons/network/material-usb.svg?raw'
import visibilityIcon from '../../assets/icons/security/material-visibility.svg?raw'
import visibilityOffIcon from '../../assets/icons/security/material-visibility_off.svg?raw'
import { parseSvgIconAsset } from './svg-icon-asset'

const sources = {
  add: addIcon,
  add_comment: addCommentIcon,
  admin_panel_settings: adminPanelSettingsIcon,
  apps: appsIcon,
  arrow_back: arrowBackIcon,
  arrow_forward: arrowForwardIcon,
  auto_awesome: autoAwesomeIcon,
  check: checkIcon,
  chevron_right: chevronRightIcon,
  close: closeIcon,
  cloud: cloudIcon,
  cloud_sync: cloudSyncIcon,
  dashboard: dashboardIcon,
  delete: deleteIcon,
  delete_sweep: deleteSweepIcon,
  description: descriptionIcon,
  dns: dnsIcon,
  download: downloadIcon,
  downloading: downloadingIcon,
  drag_indicator: dragIndicatorIcon,
  edit: editIcon,
  edit_note: editNoteIcon,
  error: errorIcon,
  fingerprint: fingerprintIcon,
  fit_screen: fitScreenIcon,
  folder: folderIcon,
  folder_open: folderOpenIcon,
  folder_shared: folderSharedIcon,
  forum: forumIcon,
  history: historyIcon,
  home: homeIcon,
  info: infoIcon,
  key: keyIcon,
  key_off: keyOffIcon,
  lock: lockIcon,
  more_horiz: moreHorizIcon,
  open_in_new: openInNewIcon,
  palette: paletteIcon,
  preview: previewIcon,
  progress_activity: progressActivityIcon,
  refresh: refreshIcon,
  restart_alt: restartAltIcon,
  save: saveIcon,
  save_as: saveAsIcon,
  search: searchIcon,
  settings: settingsIcon,
  settings_ethernet: settingsEthernetIcon,
  settings_suggest: settingsSuggestIcon,
  shield: shieldIcon,
  star: starIcon,
  stop: stopIcon,
  system_update_alt: systemUpdateAltIcon,
  terminal: terminalIcon,
  translate: translateIcon,
  tune: tuneIcon,
  upload_file: uploadFileIcon,
  usb: usbIcon,
  visibility: visibilityIcon,
  visibility_off: visibilityOffIcon
} as const

export type MaterialIconName = keyof typeof sources

export const MATERIAL_ICON_ASSETS = Object.fromEntries(
  Object.entries(sources).map(([name, source]) => [name, parseSvgIconAsset(source)])
) as Record<MaterialIconName, { viewBox: string; content: string }>
