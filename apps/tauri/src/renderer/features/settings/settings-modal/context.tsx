import { createContext, use, type ReactNode } from 'react'

const SettingsModalContext = createContext<object | null>(null)

export function SettingsModalProvider({ children, value }: { children: ReactNode; value: object }) {
  return <SettingsModalContext value={value}>{children}</SettingsModalContext>
}

export function useSettingsModalContext<T extends object>(): T {
  const value = use(SettingsModalContext)
  if (!value) {
    throw new Error('SettingsModal panels must be rendered inside SettingsModalProvider')
  }
  return value as T
}
