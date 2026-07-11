export type EditorEncodingOption = {
  label: string
  value: string
}

export const EDITOR_ENCODINGS: EditorEncodingOption[] = [
  { label: 'Unicode (UTF-8)', value: 'utf-8' },
  { label: 'Unicode (UTF-8 with BOM)', value: 'utf-8-bom' },
  { label: 'Unicode (UTF-16 LE)', value: 'utf-16le' },
  { label: 'Unicode (UTF-16 BE)', value: 'utf-16be' },
  { label: '简体中文 (GB18030)', value: 'gb18030' },
  { label: '简体中文 (GBK)', value: 'gbk' },
  { label: '繁体中文 (Big5)', value: 'big5' },
  { label: '日文 (Shift_JIS)', value: 'shift_jis' },
  { label: '日文 (EUC-JP)', value: 'euc-jp' },
  { label: '日文 (ISO-2022-JP)', value: 'iso-2022-jp' },
  { label: '韩语 (EUC-KR)', value: 'euc-kr' },
  { label: '韩语 (CP949)', value: 'cp949' },
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
