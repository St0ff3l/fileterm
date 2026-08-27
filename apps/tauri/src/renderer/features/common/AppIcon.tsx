export type AppIconName =
  | 'brand'
  | 'terminal'
  | 'telnet'
  | 'grid'
  | 'menu'
  | 'server'
  | 'connections'
  | 'key'
  | 'folder'
  | 'file'
  | 'archive'
  | 'video'
  | 'audio'
  | 'image'
  | 'document'
  | 'spreadsheet'
  | 'presentation'
  | 'config-file'
  | 'database'
  | 'cloud'
  | 'font-file'
  | 'package'
  | 'terminal-file'
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
  | 'message'
  | 'eye'
  | 'shield'
  | 'shield-check'
  | 'chevron-up'
  | 'chevron-down'
  | 'chevron-right'
  | 'arrow-up'
  | 'arrow-down'
  | 'drag-handle'
  | 'check'
  | 'play'
  | 'close'

export function AppIcon({
  name,
  size = 14,
  strokeWidth = 1.8,
  className
}: {
  name: AppIconName
  size?: number
  strokeWidth?: number
  className?: string
}) {
  const commonProps = {
    fill: 'none',
    stroke: 'currentColor',
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
    strokeWidth
  }

  return (
    <svg
      aria-hidden="true"
      className={['app-icon', `app-icon-${name}`, className].filter(Boolean).join(' ')}
      height={size}
      viewBox="0 0 16 16"
      width={size}
    >
      {name === 'brand' || name === 'terminal' ? (
        <>
          <rect {...commonProps} x="2.25" y="2.5" width="11.5" height="11" rx="2" />
          <path {...commonProps} d="m5.2 5.7 2 1.8-2 1.8" />
          <path {...commonProps} d="M8.5 10.3h3" />
        </>
      ) : null}
      {name === 'telnet' ? (
        <>
          <rect {...commonProps} x="2.25" y="2.5" width="11.5" height="8.6" rx="1.8" />
          <path {...commonProps} d="m4.8 5.4 1.8 1.4-1.8 1.4" />
          <path {...commonProps} d="M8.2 8.2h2.8" />
          <path {...commonProps} d="M8 11.1v2.4M5.2 13.5h5.6" />
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
      {name === 'key' ? (
        <>
          <circle {...commonProps} cx="5.25" cy="8" r="2.5" />
          <path {...commonProps} d="M7.75 8h5.75M10.6 8v2M12.7 8v1.5" />
        </>
      ) : null}
      {name === 'folder' ? (
        <path
          {...commonProps}
          d="M2.5 4.5h3l1.4 1.6h6.6v5.8a1.1 1.1 0 0 1-1.1 1.1H3.6a1.1 1.1 0 0 1-1.1-1.1V5.6a1.1 1.1 0 0 1 1.1-1.1Z"
        />
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
      {name === 'spreadsheet' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13M6.2 7.7h4.6v3.8H6.2zM8.5 7.7v3.8M6.2 9.6h4.6" />
        </>
      ) : null}
      {name === 'presentation' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13M6.2 7.4h4.6v3H6.2zM8.5 10.4v1.4M7.2 11.8h2.6" />
        </>
      ) : null}
      {name === 'config-file' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13M6.2 8h4.6M6.2 10.5h4.6M7.4 7.2v1.6M9.6 9.7v1.6" />
        </>
      ) : null}
      {name === 'database' ? (
        <>
          <ellipse {...commonProps} cx="8" cy="4.2" rx="4.5" ry="2" />
          <path
            {...commonProps}
            d="M3.5 4.2v3.8c0 1.1 2 2 4.5 2s4.5-.9 4.5-2V4.2M3.5 8v3.8c0 1.1 2 2 4.5 2s4.5-.9 4.5-2V8"
          />
        </>
      ) : null}
      {name === 'cloud' ? (
        <path {...commonProps} d="M4.5 12.5h7a3 3 0 0 0 .7-5.9 4 4 0 0 0-7.4-1.2 3 3 0 0 0-.3 7.1Z" />
      ) : null}
      {name === 'font-file' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13M6.2 11.5 8.3 7l2.1 4.5M6.9 10h2.8" />
        </>
      ) : null}
      {name === 'package' ? (
        <>
          <path {...commonProps} d="m8 2.2 5 2.7v6.2l-5 2.7-5-2.7V4.9l5-2.7Z" />
          <path {...commonProps} d="m3.3 5.1 4.7 2.6 4.7-2.6M8 7.7v6M5.5 3.6l4.8 2.6" />
        </>
      ) : null}
      {name === 'terminal-file' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13m-6.7 2 1.5 1.4-1.5 1.4M8.8 11h2" />
        </>
      ) : null}
      {name === 'pdf' ? (
        <>
          <path {...commonProps} d="M5 2.5h4.5L13 6v7a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-9.5a1 1 0 0 1 1-1Z" />
          <path {...commonProps} d="M9.5 2.5V6H13" />
          <path
            {...commonProps}
            d="M6.2 10.7V7.4h1.2a.9.9 0 1 1 0 1.8H6.2m2.2 1.5V7.4h.7a1.6 1.6 0 0 1 0 3.3h-.7m2-.1V7.4h1.4"
          />
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
      {name === 'flash' ? <path {...commonProps} d="M8.9 1.8 3.8 8h3l-.7 6.2L12.2 8h-3.1l-.2-6.2Z" /> : null}
      {name === 'copy' ? (
        <g
          fill="currentColor"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={strokeWidth * 16}
          transform="scale(0.015625)"
        >
          <path d="M761.088 715.3152a38.7072 38.7072 0 1 1 0-77.4144 37.4272 37.4272 0 0 0 37.4272-37.4272V265.0112a37.4272 37.4272 0 0 0-37.4272-37.4272H425.6256a37.4272 37.4272 0 0 0-37.4272 37.4272 38.7072 38.7072 0 1 1-77.4144 0 115.0976 115.0976 0 0 1 114.8416-114.8416h335.4624a115.0976 115.0976 0 0 1 114.8416 114.8416v335.4624a115.0976 115.0976 0 0 1-114.8416 114.8416z" />
          <path d="M589.4656 883.0976H268.1856a121.1392 121.1392 0 0 1-121.2928-121.2928v-322.56a121.1392 121.1392 0 0 1 121.2928-121.344h321.28a121.1392 121.1392 0 0 1 121.2928 121.2928v322.56c1.28 67.1232-54.1696 121.344-121.2928 121.344zM268.1856 395.3152a43.52 43.52 0 0 0-43.8784 43.8784v322.56a43.52 43.52 0 0 0 43.8784 43.8784h321.28a43.52 43.52 0 0 0 43.8784-43.8784v-322.56a43.52 43.52 0 0 0-43.8784-43.8784z" />
        </g>
      ) : null}
      {name === 'paste' ? (
        <g
          fill="currentColor"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={strokeWidth * 16}
          transform="scale(0.015625)"
        >
          <path d="M768 348h-28v-28c0-90.44-73.56-164-164-164h-2.72c-14.48-37.4-50.8-64-93.28-64H352c-42.44 0-78.8 26.6-93.28 64H256c-90.44 0-164 73.56-164 164v448c0 90.44 73.56 164 164 164h368c1.72 0 3.44-0.16 5.08-0.4 3.6 0.24 7.24 0.4 10.92 0.4h128c90.44 0 164-73.56 164-164v-256c0-90.44-73.56-164-164-164zM352 164h128c15.44 0 28 12.56 28 28s-12.56 28-28 28H352c-15.44 0-28-12.56-28-28s12.56-28 28-28z m152.32 696H256c-50.72 0-92-41.28-92-92V320c0-50.72 41.28-92 92-92h2.72C273.2 265.4 309.52 292 352 292h128c42.44 0 78.8-26.6 93.28-64h2.72c50.72 0 92 41.28 92 92v28h-28c-90.44 0-164 73.56-164 164v256c0 34.08 10.44 65.76 28.32 92zM860 768c0 50.72-41.28 92-92 92h-128c-50.72 0-92-41.28-92-92v-256c0-50.72 41.28-92 92-92h128c50.72 0 92 41.28 92 92v256z" />
        </g>
      ) : null}
      {name === 'message' ? (
        <path
          {...commonProps}
          d="M3 3.5h10a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H8l-3.5 2.5v-2.5H3a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1Z"
        />
      ) : null}
      {name === 'eye' ? (
        <>
          <path {...commonProps} d="M1.8 8s2.1-3.5 6.2-3.5S14.2 8 14.2 8 12.1 11.5 8 11.5 1.8 8 1.8 8Z" />
          <circle {...commonProps} cx="8" cy="8" r="1.7" />
        </>
      ) : null}
      {name === 'shield' || name === 'shield-check' ? (
        <path {...commonProps} d="M8 1.9 13 3.8v3.5c0 3.1-2 5.7-5 6.8-3-1.1-5-3.7-5-6.8V3.8L8 1.9Z" />
      ) : null}
      {name === 'shield-check' ? <path {...commonProps} d="m5.2 8 1.7 1.7 3.8-3.8" /> : null}
      {name === 'chevron-up' ? <path {...commonProps} d="m3.5 10 4.5-4.5 4.5 4.5" /> : null}
      {name === 'chevron-down' ? <path {...commonProps} d="m3.5 6 4.5 4.5 4.5-4.5" /> : null}
      {name === 'chevron-right' ? <path {...commonProps} d="m6 3.5 4.5 4.5L6 12.5" /> : null}
      {name === 'arrow-up' ? <path {...commonProps} d="M8 13V3m-4 4 4-4 4 4" /> : null}
      {name === 'arrow-down' ? <path {...commonProps} d="M8 3v10m-4-4 4 4 4-4" /> : null}
      {name === 'drag-handle' ? (
        <path {...commonProps} d="M5 4.5h.01M5 8h.01M5 11.5h.01M11 4.5h.01M11 8h.01M11 11.5h.01" strokeWidth={3} />
      ) : null}
      {name === 'check' ? <path {...commonProps} d="m3.2 8.2 3.1 3.1 6.5-6.6" /> : null}
      {name === 'play' ? <path {...commonProps} d="M5.5 3.5v9l6.5-4.5z" /> : null}
      {name === 'close' ? <path {...commonProps} d="M3.5 3.5l9 9M12.5 3.5l-9 9" /> : null}
    </svg>
  )
}
