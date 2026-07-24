// 类型化的应用层 UI 事件总线。
//
// 这里收敛了原本散落在 useFileOperations / TerminalView / TerminalDock /
// useWorkspaceTabs / FileManager 等组件里的 `fileterm:*` 字符串事件。
// 收益有两点：
//   1. 事件名拼写错误在编译期暴露，不会静默丢失监听。
//   2. detail 类型与事件名绑定，dispatch / listen 两侧都无需手动 `as
//      CustomEvent` 断言。
//
// 注意：这是 UI 层 pub/sub，不替代 Rust commands/events 边界（硬边界 #6
// 针对的是系统能力走 Rust 的链路，UI 内部事件不在此列）。

export const APP_EVENT = {
  /** Tauri 原生拖拽悬停（Finder/Explorer 拖入窗口时持续触发）。 */
  tauriNativeDragOver: 'fileterm:tauri-native-drag-over',
  /** Tauri 原生拖放落下，携带绝对路径。 */
  tauriNativeDrop: 'fileterm:tauri-native-drop',
  /** 远端文件区被 DOM 拖入事件标记为可投放目标。 */
  tauriRemoteDragOver: 'fileterm:tauri-remote-dragover',
  /** 请求聚焦指定 tab 的终端。detail 为 tabId。 */
  focusTerminal: 'fileterm:focus-terminal',
  /** 请求当前终端执行复制。 */
  terminalCopy: 'fileterm:terminal-copy',
  /** 请求当前终端执行粘贴。 */
  terminalPaste: 'fileterm:terminal-paste',
  /** 请求当前终端切换查找栏。 */
  terminalFind: 'fileterm:terminal-find'
} as const

export type AppEventName = (typeof APP_EVENT)[keyof typeof APP_EVENT]

/** 事件名 → detail 类型映射。无 detail 的事件用 `never`。 */
export interface AppEventDetailMap {
  [APP_EVENT.tauriNativeDragOver]: { position: { x: number; y: number } }
  [APP_EVENT.tauriNativeDrop]: {
    paths: string[]
    consume: () => void
    position: { x: number; y: number }
  }
  [APP_EVENT.tauriRemoteDragOver]: never
  [APP_EVENT.focusTerminal]: string
  [APP_EVENT.terminalCopy]: never
  [APP_EVENT.terminalPaste]: never
  [APP_EVENT.terminalFind]: never
}

/** 派发一个类型化的应用事件。`detail` 为 `never` 的事件不需要传第二参。 */
export function dispatchAppEvent<K extends AppEventName>(
  name: K,
  ...detail: AppEventDetailMap[K] extends never ? [] : [AppEventDetailMap[K]]
): void {
  const event = detail.length === 0 ? new Event(name) : new CustomEvent(name, { detail: detail[0] })
  window.dispatchEvent(event)
}

/** 监听一个类型化的应用事件，返回取消监听的函数。 */
export function onAppEvent<K extends AppEventName>(
  name: K,
  listener: (detail: AppEventDetailMap[K]) => void
): () => void {
  const handler = (event: Event) => {
    if (event instanceof CustomEvent) {
      listener(event.detail as AppEventDetailMap[K])
    } else {
      // 无 detail 的事件：detail 类型为 `never`，listener 不应使用参数。
      // 双重断言绕过 `undefined -> never` 的 TS2352 检查，运行时该分支
      // 只会触发于 `Event`（非 `CustomEvent`），对应 `never` 的事件名。
      listener(undefined as unknown as AppEventDetailMap[K])
    }
  }
  window.addEventListener(name, handler)
  return () => window.removeEventListener(name, handler)
}
