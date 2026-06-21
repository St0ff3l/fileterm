export type AppIconName =
  | 'brand'
  | 'grid'
  | 'menu'
  | 'server'
  | 'connections'
  | 'folder'
  | 'file'
  | 'archive'
  | 'video'
  | 'audio'
  | 'image'
  | 'document'
  | 'pdf'
  | 'code'
  | 'disk'
  | 'history'
  | 'refresh'
  | 'search'
  | 'plus'
  | 'edit'
  | 'trash'
  | 'upload'
  | 'download'
  | 'flash'
  | 'copy'
  | 'paste'
  | 'chevron-up'
  | 'chevron-down'
  | 'play'
  | 'close'


export function AppIcon({
  name,
  size = 14
}: {
  name: AppIconName
  size?: number
}) {
  const commonProps = {
    fill: 'none',
    stroke: 'currentColor',
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
    strokeWidth: 1.8
  }

  return (
    <svg aria-hidden="true" className={`app-icon app-icon-${name}`} height={size} viewBox="0 0 16 16" width={size}>
      {name === 'brand' ? (
        <>
          <rect {...commonProps} x="2.25" y="2.5" width="11.5" height="11" rx="2" />
          <path {...commonProps} d="m5.2 5.7 2 1.8-2 1.8" />
          <path {...commonProps} d="M8.5 10.3h3" />
        </>
      ) : null}
      {name === 'grid' ? (
        <>
          <rect {...commonProps} x="2.25" y="2.25" width="4.5" height="4.5" />
          <rect {...commonProps} x="9.25" y="2.25" width="4.5" height="4.5" />
          <rect {...commonProps} x="2.25" y="9.25" width="4.5" height="4.5" />
          <rect {...commonProps} x="9.25" y="9.25" width="4.5" height="4.5" />
        </>
      ) : null}
      {name === 'menu' ? (
        <>
          <path {...commonProps} d="M3 4.5h10" />
          <path {...commonProps} d="M3 8h10" />
          <path {...commonProps} d="M3 11.5h10" />
        </>
      ) : null}
      {name === 'server' ? (
        <>
          <rect {...commonProps} x="2.5" y="2.5" width="11" height="4" rx="1.2" />
          <rect {...commonProps} x="2.5" y="9.5" width="11" height="4" rx="1.2" />
          <path {...commonProps} d="M4.5 4.5h.01M4.5 11.5h.01" />
          <path {...commonProps} d="M8 6.5v3" />
        </>
      ) : null}
      {name === 'connections' ? (
        <>
          <rect {...commonProps} x="2.8" y="3" width="4.2" height="4.2" rx="1" />
          <rect {...commonProps} x="9" y="3" width="4.2" height="4.2" rx="1" />
          <rect {...commonProps} x="5.9" y="9" width="4.2" height="4.2" rx="1" />
          <path {...commonProps} d="M7 5.1h2" />
          <path {...commonProps} d="M5.2 7.2 6.8 9" />
          <path {...commonProps} d="M10.8 7.2 9.2 9" />
        </>
      ) : null}
      {name === 'folder' ? (
        <path {...commonProps} d="M2.5 4.5h3l1.4 1.6h6.6v5.8a1.1 1.1 0 0 1-1.1 1.1H3.6a1.1 1.1 0 0 1-1.1-1.1V5.6a1.1 1.1 0 0 1 1.1-1.1Z" />
      ) : null}
      {name === 'file' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13" />
        </>
      ) : null}
      {name === 'archive' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13" />
          <path {...commonProps} d="M8 4.1v1.1M8 6.2v1.1M8 8.3v1.1M7 10h2" />
        </>
      ) : null}
      {name === 'video' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13" />
          <path {...commonProps} d="m6.9 7.1 3.1 1.9-3.1 1.9Z" />
        </>
      ) : null}
      {name === 'audio' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13" />
          <path {...commonProps} d="M9.5 7.1v3.1a1 1 0 1 1-1-.9h1" />
          <path {...commonProps} d="M9.5 7.1 7.7 7.7" />
        </>
      ) : null}
      {name === 'image' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13" />
          <path {...commonProps} d="m6 11 1.9-2 1.4 1.4 1.7-1.8 1 1.1" />
          <path {...commonProps} d="M7 7.2h.01" />
        </>
      ) : null}
      {name === 'document' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13" />
          <path {...commonProps} d="M6.4 7.6h3.9M6.4 9.2h3.9M6.4 10.8h2.7" />
        </>
      ) : null}
      {name === 'pdf' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13" />
          <path {...commonProps} d="M6.2 10.7V7.4h1.2a.9.9 0 1 1 0 1.8H6.2m2.2 1.5V7.4h.7a1.6 1.6 0 0 1 0 3.3h-.7m2-.1V7.4h1.4" />
        </>
      ) : null}
      {name === 'code' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13" />
          <path {...commonProps} d="m7.1 8.1-1.4 1.2 1.4 1.2M9.1 8.1l1.4 1.2-1.4 1.2" />
        </>
      ) : null}
      {name === 'disk' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13" />
          <circle {...commonProps} cx="8.2" cy="9.4" r="2.2" />
          <path {...commonProps} d="M8.2 9.4h.01" />
        </>
      ) : null}
      {name === 'history' ? (
        <>
          <path {...commonProps} d="M3.2 8a4.8 4.8 0 1 0 1.4-3.4" />
          <path {...commonProps} d="M3 3.5v2.8h2.8" />
        </>
      ) : null}
      {name === 'refresh' ? (
        <>
          <path {...commonProps} d="M12.8 6A4.9 4.9 0 0 0 4.4 4.2" />
          <path {...commonProps} d="M4.2 2.9v2.8H7" />
          <path {...commonProps} d="M3.2 10A4.9 4.9 0 0 0 11.6 11.8" />
          <path {...commonProps} d="M11.8 13.1v-2.8H9" />
        </>
      ) : null}
      {name === 'search' ? (
        <>
          <circle {...commonProps} cx="7" cy="7" r="4" />
          <path {...commonProps} d="m10 10 3.2 3.2" />
        </>
      ) : null}
      {name === 'plus' ? (
        <>
          <path {...commonProps} d="M8 3.5v9" />
          <path {...commonProps} d="M3.5 8h9" />
        </>
      ) : null}
      {name === 'edit' ? (
        <>
          <path {...commonProps} d="M3.1 11.9 3.8 9l5.9-5.9a1.6 1.6 0 0 1 2.2 2.2L6 11.2l-2.9.7Z" />
          <path {...commonProps} d="m8.8 4 2.2 2.2" />
        </>
      ) : null}
      {name === 'trash' ? (
        <>
          <path {...commonProps} d="M3.2 4.6h9.6" />
          <path {...commonProps} d="M6.2 4.6V3.3h3.6v1.3" />
          <path {...commonProps} d="M4.7 6.4 5.2 13h5.6l.5-6.6" />
          <path {...commonProps} d="M7 7.6v3.6M9 7.6v3.6" />
        </>
      ) : null}
      {name === 'download' ? (
        <>
          <path {...commonProps} d="M8 3.5v7.3" />
          <path {...commonProps} d="M4.7 7.5 8 10.8l3.3-3.3" />
          <path {...commonProps} d="M3 12.5h10" />
        </>
      ) : null}
      {name === 'upload' ? (
        <>
          <path {...commonProps} d="M8 10.8V3.5" />
          <path {...commonProps} d="M4.7 6.8 8 3.5l3.3 3.3" />
          <path {...commonProps} d="M3 12.5h10" />
        </>
      ) : null}
      {name === 'flash' ? (
        <path {...commonProps} d="M8.9 1.8 3.8 8h3l-.7 6.2L12.2 8h-3.1l-.2-6.2Z" />
      ) : null}
      {name === 'copy' ? (
        <>
          <rect {...commonProps} x="4.5" y="4.5" width="8" height="8" rx="1.5" />
          <path {...commonProps} d="M11.5 2.5h-5a2 2 0 0 0-2 2v5" />
        </>
      ) : null}
      {name === 'paste' ? (
        <>
          <path {...commonProps} d="M5.5 3h5A1.5 1.5 0 0 1 12 4.5v8a1.5 1.5 0 0 1-1.5 1.5h-5A1.5 1.5 0 0 1 4 12.5v-8A1.5 1.5 0 0 1 5.5 3Z" />
          <path {...commonProps} d="M6.5 2h3v1.5h-3z" />
        </>
      ) : null}
      {name === 'chevron-up' ? (
        <path {...commonProps} d="m3.5 10 4.5-4.5 4.5 4.5" />
      ) : null}
      {name === 'chevron-down' ? (
        <path {...commonProps} d="m3.5 6 4.5 4.5 4.5-4.5" />
      ) : null}
      {name === 'play' ? (
        <path {...commonProps} d="M5.5 3.5v9l6.5-4.5z" />
      ) : null}
      {name === 'close' ? (
        <path {...commonProps} d="M3.5 3.5l9 9M12.5 3.5l-9 9" />
      ) : null}
    </svg>
  )
}
