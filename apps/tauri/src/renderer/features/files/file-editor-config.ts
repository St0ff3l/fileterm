import { t } from '../../i18n'

export type EditorEncodingOption = {
  label: string
  value: string
}

export const EDITOR_ENCODINGS: EditorEncodingOption[] = [
  { label: 'Unicode (UTF-8)', value: 'utf-8' },
  { label: 'Unicode (UTF-8 with BOM)', value: 'utf-8-bom' },
  { label: 'Unicode (UTF-16 LE)', value: 'utf-16le' },
  { label: 'Unicode (UTF-16 BE)', value: 'utf-16be' },
  { label: 'Simplified Chinese (GB18030)', value: 'gb18030' },
  { label: 'Simplified Chinese (GBK)', value: 'gbk' },
  { label: 'Traditional Chinese (Big5)', value: 'big5' },
  { label: 'Japanese (Shift_JIS)', value: 'shift_jis' },
  { label: 'Japanese (EUC-JP)', value: 'euc-jp' },
  { label: 'Japanese (ISO-2022-JP)', value: 'iso-2022-jp' },
  { label: 'Korean (EUC-KR)', value: 'euc-kr' },
  { label: 'Korean (CP949)', value: 'cp949' },
  { label: 'Western (Windows-1252)', value: 'windows-1252' },
  { label: 'Western (ISO-8859-1)', value: 'iso-8859-1' },
  { label: 'Cyrillic (Windows-1251)', value: 'windows-1251' }
]

export type EditorLanguageOption = {
  id: string
  label: string
}

export function findEncodingOption(value: string) {
  return EDITOR_ENCODINGS.find((option) => option.value === value) ?? EDITOR_ENCODINGS[0]
}

export function getEditorEncodingLabel(value: string) {
  const option = findEncodingOption(value)
  const family =
    option.value === 'gb18030' || option.value === 'gbk'
      ? t.encodingSimplifiedChinese
      : option.value === 'big5'
        ? t.encodingTraditionalChinese
        : option.value === 'shift_jis' || option.value === 'euc-jp' || option.value === 'iso-2022-jp'
          ? t.encodingJapanese
          : option.value === 'euc-kr' || option.value === 'cp949'
            ? t.encodingKorean
            : null
  if (!family) return option.label
  const encoding = option.label.match(/\(([^)]+)\)/)?.[1] ?? option.value
  return `${family} (${encoding})`
}

export function sortEditorLanguages(languages: Array<{ id: string; aliases?: string[] }>): EditorLanguageOption[] {
  return languages
    .map((language) => ({
      id: language.id,
      label: language.aliases?.[0] ?? language.id
    }))
    .filter((language, index, list) => {
      return list.findIndex((item) => item.id === language.id) === index
    })
    .sort((a, b) => a.label.localeCompare(b.label))
}
