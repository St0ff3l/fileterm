import { useMemo, useState } from 'react'
import { useSshKeyLibrary } from '../../hooks/useSshKeyLibrary'

export function SshKeyManagerPage() {
  const { keys, loading, error, importKey, updateNote, deleteKey } = useSshKeyLibrary()
  const [query, setQuery] = useState('')
  const [busy, setBusy] = useState(false)

  const visibleKeys = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase()
    if (!normalized) return keys
    return keys.filter((key) =>
      [key.name, key.note, key.algorithm, key.fingerprint].some((value) =>
        value?.toLocaleLowerCase().includes(normalized)
      )
    )
  }, [keys, query])

  const handleImport = async () => {
    setBusy(true)
    try {
      await importKey('')
    } catch {
      // useSshKeyLibrary 已将可展示错误写入 error 状态。
    } finally {
      setBusy(false)
    }
  }

  const handleEditNote = async (keyId: string, currentNote = '') => {
    const note = window.prompt('修改备注', currentNote)
    if (note === null) return
    await updateNote(keyId, note)
  }

  const handleDelete = async (keyId: string, name: string) => {
    if (!window.confirm(`确定删除密钥“${name}”吗？此操作不会删除原始文件。`)) return
    await deleteKey(keyId)
  }

  return (
    <section className="ssh-key-manager-page">
      <header className="ssh-key-manager-header">
        <div>
          <h2>密钥管理</h2>
          <p>集中管理 SSH 私钥。私钥副本由 FileTerm 托管，连接只保存密钥引用。</p>
        </div>
        <button className="primary-button" disabled={busy} onClick={() => void handleImport()} type="button">
          <span aria-hidden="true" className="material-symbols-outlined">
            key
          </span>
          {busy ? '正在导入…' : '导入私钥'}
        </button>
      </header>

      <div className="ssh-key-manager-toolbar">
        <label className="connection-manager-search">
          <span aria-hidden="true" className="material-symbols-outlined">
            search
          </span>
          <input
            placeholder="搜索名称、备注、算法或指纹"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <span>{keys.length} 个密钥</span>
      </div>

      {error ? <div className="modal-error ssh-key-manager-error">{error}</div> : null}

      <div className="ssh-key-table-shell">
        <table className="ssh-key-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>算法 / 指纹</th>
              <th>备注</th>
              <th>导入时间</th>
              <th>引用</th>
              <th aria-label="操作" />
            </tr>
          </thead>
          <tbody>
            {visibleKeys.map((key) => (
              <tr key={key.id}>
                <td>
                  <strong>{key.name}</strong>
                  <span className="ssh-key-status">{key.encrypted ? '已加密' : '未加密'}</span>
                </td>
                <td className="ssh-key-fingerprint">
                  <span>{key.algorithm}</span>
                  <code title={key.fingerprint}>{shortFingerprint(key.fingerprint)}</code>
                </td>
                <td>{key.note || '—'}</td>
                <td>
                  {new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(
                    key.importedAt
                  )}
                </td>
                <td>{key.usageCount}</td>
                <td className="ssh-key-actions">
                  <button
                    className="flat-button compact"
                    onClick={() => void handleEditNote(key.id, key.note)}
                    type="button"
                  >
                    修改备注
                  </button>
                  <button
                    className="flat-button compact danger"
                    disabled={key.usageCount > 0}
                    title={key.usageCount > 0 ? '该密钥仍被连接引用，无法删除' : '删除密钥'}
                    onClick={() => void handleDelete(key.id, key.name)}
                    type="button"
                  >
                    删除
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {!loading && visibleKeys.length === 0 ? (
          <div className="ssh-key-empty">
            <span aria-hidden="true" className="material-symbols-outlined">
              key_off
            </span>
            <strong>{query ? '没有匹配的密钥' : '尚未导入私钥'}</strong>
            <span>{query ? '尝试其他搜索词。' : '导入后即可在 SSH 连接中复用。'}</span>
          </div>
        ) : null}
        {loading ? <div className="ssh-key-empty">正在加载密钥列表…</div> : null}
      </div>
    </section>
  )
}

function shortFingerprint(fingerprint: string) {
  return fingerprint.length > 34 ? `${fingerprint.slice(0, 18)}…${fingerprint.slice(-12)}` : fingerprint
}
