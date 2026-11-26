import { useId, useState } from 'react'
import { PanelActionButton } from '../../../components/common/panel-action-button/panel-action-button'
import { Textarea } from '../../../components/common/input/input'
import { t } from '../../../i18n'
import { TOPOLOGY_MEMO_LIMIT } from '../server-topology-model'

export function NodeMemo({ memo, onSave }: { memo: string; onSave(value: string): void }) {
  const id = useId()
  const [draft, setDraft] = useState(memo)

  return (
    <form
      className="server-topology__memo"
      onSubmit={(event) => {
        event.preventDefault()
        if (draft !== memo) onSave(draft)
      }}
    >
      <label htmlFor={id}>{t.serverTopologyMemo}</label>
      <Textarea
        id={id}
        maxLength={TOPOLOGY_MEMO_LIMIT}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
            event.preventDefault()
            if (draft !== memo) onSave(draft)
          }
        }}
        placeholder={t.serverTopologyMemoPlaceholder}
        rows={5}
        value={draft}
      />
      <PanelActionButton disabled={draft === memo} type="submit">
        {t.save}
      </PanelActionButton>
    </form>
  )
}
