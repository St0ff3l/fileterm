import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import * as jsx from 'react/jsx-runtime'
import ts from 'typescript'

const exports = {}
const body = {}
const dependencies = {
  react: {
    useRef: (val) => ({ current: val ?? null }),
    useState: (init) => [typeof init === 'function' ? init() : init, () => {}],
    useEffect: () => {},
    useCallback: (fn) => fn
  },
  'react/jsx-runtime': jsx,
  'react-dom': { createPortal: (node, container) => ({ node, container }) },
  '../../../features/common/app-icon': {
    AppIcon: (props) => ({ type: 'AppIcon', props })
  },
  '../../../features/common/close-button': {
    CloseButton: (props) => ({ type: 'CloseButton', props })
  },
  '../../../features/common/feedback-text': {
    FeedbackText: (props) => ({ type: 'FeedbackText', props })
  },
  '../../../features/common/vertical-scrollbar': {
    VerticalScrollbar: (props) => ({ type: 'VerticalScrollbar', props })
  },
  '../../../i18n': {
    t: {
      closeTab: '关闭',
      feedbackSuccess: '操作成功',
      feedbackError: '操作失败',
      feedbackWarning: '操作警告',
      feedbackInfo: '提示'
    }
  }
}
const source = readFileSync(
  new URL('../../src/renderer/components/common/feedback-bubble/feedback-bubble.tsx', import.meta.url),
  'utf8'
)
vm.runInNewContext(
  ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2022 }
  }).outputText,
  { exports, document: { body }, require: (name) => dependencies[name] ?? {} }
)

const findChild = (node, predicate) => {
  if (!node) return null
  if (predicate(node)) return node
  const list = Array.isArray(node.props?.children) ? node.props.children.flat() : [node.props?.children]
  for (const child of list) {
    const found = findChild(child, predicate)
    if (found) return found
  }
  return null
}

test('operation feedback is absent when there is no result', () => {
  assert.equal(exports.FeedbackBubble({ message: null }), null)
  assert.equal(exports.FeedbackBubble({ message: '' }), null)
})

test('floating feedback stays outside the panel and preserves full text and status', () => {
  for (const tone of ['success', 'warning', 'error']) {
    const message = 'A complete operation result with detailed information. '.repeat(20)
    const portal = exports.FeedbackBubble({ message, tone })
    assert.equal(portal.container, body)
    assert.ok(portal.node.props.className.includes('feedback-bubble'))
    assert.ok(portal.node.props.className.includes(`feedback-bubble--${tone}`))

    const content = findChild(portal.node, (c) => c?.props?.className === 'feedback-bubble__content')
    assert.ok(content, 'feedback-bubble__content should exist')
    const feedback = content.props.children
    assert.equal(feedback.props.message, message)
    assert.equal(feedback.props.tone, tone)

    const titleNode = findChild(portal.node, (c) => c?.props?.className === 'feedback-bubble__title')
    assert.ok(titleNode, 'feedback-bubble__title should exist')

    const closeBtn = findChild(portal.node, (c) => c?.props?.className?.includes('feedback-bubble__close-button'))
    assert.ok(closeBtn, 'CloseButton should exist')
  }
})
