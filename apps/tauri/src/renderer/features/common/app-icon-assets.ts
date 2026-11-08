import starIcon from '../../assets/icons/system/app-star.svg?raw'
import terminalIcon from '../../assets/icons/network/app-terminal.svg?raw'
import telnetIcon from '../../assets/icons/network/app-telnet.svg?raw'
import gridIcon from '../../assets/icons/navigation/app-grid.svg?raw'
import menuIcon from '../../assets/icons/system/app-menu.svg?raw'
import serverIcon from '../../assets/icons/network/app-server.svg?raw'
import connectionsIcon from '../../assets/icons/network/app-connections.svg?raw'
import keyIcon from '../../assets/icons/security/app-key.svg?raw'
import lockIcon from '../../assets/icons/security/app-lock.svg?raw'
import lockScreenIcon from '../../assets/icons/security/app-lock-screen.svg?raw'
import folderIcon from '../../assets/icons/files/app-folder.svg?raw'
import fileIcon from '../../assets/icons/files/app-file.svg?raw'
import archiveIcon from '../../assets/icons/files/app-archive.svg?raw'
import videoIcon from '../../assets/icons/files/app-video.svg?raw'
import audioIcon from '../../assets/icons/files/app-audio.svg?raw'
import imageIcon from '../../assets/icons/files/app-image.svg?raw'
import documentIcon from '../../assets/icons/files/app-document.svg?raw'
import spreadsheetIcon from '../../assets/icons/files/app-spreadsheet.svg?raw'
import presentationIcon from '../../assets/icons/files/app-presentation.svg?raw'
import configFileIcon from '../../assets/icons/files/app-config-file.svg?raw'
import databaseIcon from '../../assets/icons/files/app-database.svg?raw'
import cloudIcon from '../../assets/icons/network/app-cloud.svg?raw'
import fontFileIcon from '../../assets/icons/files/app-font-file.svg?raw'
import packageIcon from '../../assets/icons/files/app-package.svg?raw'
import terminalFileIcon from '../../assets/icons/files/app-terminal-file.svg?raw'
import pdfIcon from '../../assets/icons/files/app-pdf.svg?raw'
import codeIcon from '../../assets/icons/files/app-code.svg?raw'
import diskIcon from '../../assets/icons/files/app-disk.svg?raw'
import historyIcon from '../../assets/icons/system/app-history.svg?raw'
import callLogIcon from '../../assets/icons/system/app-call-log.svg?raw'
import refreshIcon from '../../assets/icons/actions/app-refresh.svg?raw'
import searchIcon from '../../assets/icons/actions/app-search.svg?raw'
import plusIcon from '../../assets/icons/actions/app-plus.svg?raw'
import editIcon from '../../assets/icons/actions/app-edit.svg?raw'
import trashIcon from '../../assets/icons/actions/app-trash.svg?raw'
import transferIcon from '../../assets/icons/files/app-transfer.svg?raw'
import uploadIcon from '../../assets/icons/files/app-upload.svg?raw'
import downloadIcon from '../../assets/icons/files/app-download.svg?raw'
import flashIcon from '../../assets/icons/actions/app-flash.svg?raw'
import copyIcon from '../../assets/icons/actions/app-copy.svg?raw'
import pasteIcon from '../../assets/icons/actions/app-paste.svg?raw'
import messageIcon from '../../assets/icons/system/app-message.svg?raw'
import eyeIcon from '../../assets/icons/security/app-eye.svg?raw'
import eyeOffIcon from '../../assets/icons/security/app-eye-off.svg?raw'
import shieldIcon from '../../assets/icons/security/app-shield.svg?raw'
import shieldCheckIcon from '../../assets/icons/security/app-shield-check.svg?raw'
import chevronUpIcon from '../../assets/icons/navigation/app-chevron-up.svg?raw'
import chevronDownIcon from '../../assets/icons/navigation/app-chevron-down.svg?raw'
import chevronRightIcon from '../../assets/icons/navigation/app-chevron-right.svg?raw'
import arrowUpIcon from '../../assets/icons/navigation/app-arrow-up.svg?raw'
import arrowDownIcon from '../../assets/icons/navigation/app-arrow-down.svg?raw'
import arrowRightIcon from '../../assets/icons/navigation/app-arrow-right.svg?raw'
import dragHandleIcon from '../../assets/icons/actions/app-drag-handle.svg?raw'
import checkIcon from '../../assets/icons/actions/app-check.svg?raw'
import playIcon from '../../assets/icons/actions/app-play.svg?raw'
import infoIcon from '../../assets/icons/system/app-info.svg?raw'
import closeIcon from '../../assets/icons/navigation/app-close.svg?raw'
import windowMinimizeIcon from '../../assets/icons/navigation/app-window-minimize.svg?raw'
import windowMaximizeIcon from '../../assets/icons/navigation/app-window-maximize.svg?raw'
import windowRestoreIcon from '../../assets/icons/navigation/app-window-restore.svg?raw'

import { parseSvgIconAsset } from './svg-icon-asset'

const sources = {
  star: starIcon,
  brand: terminalIcon,
  terminal: terminalIcon,
  telnet: telnetIcon,
  grid: gridIcon,
  menu: menuIcon,
  server: serverIcon,
  connections: connectionsIcon,
  key: keyIcon,
  lock: lockIcon,
  'lock-screen': lockScreenIcon,
  folder: folderIcon,
  file: fileIcon,
  archive: archiveIcon,
  video: videoIcon,
  audio: audioIcon,
  image: imageIcon,
  document: documentIcon,
  spreadsheet: spreadsheetIcon,
  presentation: presentationIcon,
  'config-file': configFileIcon,
  database: databaseIcon,
  cloud: cloudIcon,
  'font-file': fontFileIcon,
  package: packageIcon,
  'terminal-file': terminalFileIcon,
  pdf: pdfIcon,
  code: codeIcon,
  disk: diskIcon,
  history: historyIcon,
  'call-log': callLogIcon,
  refresh: refreshIcon,
  search: searchIcon,
  plus: plusIcon,
  edit: editIcon,
  trash: trashIcon,
  transfer: transferIcon,
  upload: uploadIcon,
  download: downloadIcon,
  flash: flashIcon,
  copy: copyIcon,
  paste: pasteIcon,
  message: messageIcon,
  eye: eyeIcon,
  'eye-off': eyeOffIcon,
  shield: shieldIcon,
  'shield-check': shieldCheckIcon,
  'chevron-up': chevronUpIcon,
  'chevron-down': chevronDownIcon,
  'chevron-right': chevronRightIcon,
  'arrow-up': arrowUpIcon,
  'arrow-down': arrowDownIcon,
  'arrow-right': arrowRightIcon,
  'drag-handle': dragHandleIcon,
  check: checkIcon,
  play: playIcon,
  info: infoIcon,
  close: closeIcon,
  'window-minimize': windowMinimizeIcon,
  'window-maximize': windowMaximizeIcon,
  'window-restore': windowRestoreIcon
} as const

export type AppIconName = keyof typeof sources

export const APP_ICON_ASSETS = Object.fromEntries(
  Object.entries(sources).map(([name, source]) => [name, parseSvgIconAsset(source)])
) as Record<AppIconName, { viewBox: string; content: string }>
