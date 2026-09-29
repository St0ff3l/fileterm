import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import vm from 'node:vm'
import ts from 'typescript'
import * as shared from '@fileterm/shared'
import * as queue from '../src/renderer/hooks/ssh-interaction-queue.ts'
import {
  acceptConnectionTestHostTrust,
  invalidateChangedConnectionHostTrust
} from '../src/renderer/hooks/connection-host-trust.ts'

const draft = {
  type: 'ssh',
  name: 'Server',
  host: 'example.test',
  port: 22,
  username: 'user',
  password: 'test',
  group: 'Default',
  remotePath: '/',
  trustedHostFingerprint: ''
}
const request = {
  kind: 'host-verification',
  requestId: 'request-1',
  flowId: 'flow-1',
  tabId: 'connection-test-1',
  profileId: '',
  connectionName: 'Server',
  authenticationTarget: 'direct',
  hopIndex: 0,
  stage: 'host-key',
  sequence: 1,
  host: draft.host,
  port: draft.port,
  fingerprint: 'SHA256:accepted-key'
}

// Execute the production hooks with deterministic state and IPC boundaries.
function hookHarness(file, dependencies) {
  const state = []
  let cursor = 0
  let effects = []
  const react = {
    useState(initial) {
      const slot = cursor++
      if (!(slot in state)) state[slot] = typeof initial === 'function' ? initial() : initial
      return [
        state[slot],
        (next) => {
          state[slot] = typeof next === 'function' ? next(state[slot]) : next
        }
      ]
    },
    useRef(initial) {
      const slot = cursor++
      if (!(slot in state)) state[slot] = { current: initial }
      return state[slot]
    },
    useCallback: (callback) => callback,
    useMemo: (factory) => factory(),
    useEffect(callback, deps) {
      const slot = cursor++
      if (!state[slot] || deps.some((value, index) => !Object.is(value, state[slot][index]))) {
        state[slot] = deps
        effects.push(callback)
      }
    }
  }
  const exports = {}
  const source = readFileSync(new URL('../src/renderer/hooks/' + file + '.ts', import.meta.url), 'utf8')
  vm.runInNewContext(
    ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } })
      .outputText,
    {
      exports,
      console,
      setTimeout,
      clearTimeout,
      Error,
      require(name) {
        if (name === 'react') return react
        if (name === '@fileterm/shared') return shared
        if (name === '../i18n') return { t: {} }
        if (name in dependencies) return dependencies[name]
        throw new Error('Unexpected dependency: ' + name)
      }
    }
  )
  return (name, options) => {
    cursor = 0
    effects = []
    const result = exports[name](options)
    effects.forEach((effect) => effect())
    return result
  }
}

async function interactionFixture({
  failure,
  responseGate,
  form = draft,
  prompt = request,
  editingProfileId = null
} = {}) {
  let listener
  let currentForm = { ...form }
  const responses = []
  const errors = []
  let acceptedCount = 0
  const render = hookHarness('use-ssh-interactions', { './ssh-interaction-queue': queue })
  const options = {
    desktopApi: {
      onSshInteraction: async (callback) => {
        listener = callback
        return () => {}
      },
      showCurrentWindow: async () => {},
      resolveSshInteraction: async (_id, response) => {
        responses.push(response)
        if (responseGate) await responseGate
        if (failure) throw new Error(failure)
      }
    },
    isConnectionFormWindow: true,
    onError: (_scope, error) => errors.push(error),
    onHostTrustAccepted: (accepted) => {
      acceptedCount++
      currentForm = acceptConnectionTestHostTrust(currentForm, accepted, editingProfileId)
    }
  }
  await render('useSshInteractions', options).waitForSshInteractionListener()
  listener(prompt)
  return {
    hook: render('useSshInteractions', options),
    responses,
    errors,
    get form() {
      return currentForm
    },
    get acceptedCount() {
      return acceptedCount
    }
  }
}

async function saveForm(form, editingProfileId = null) {
  const saved = []
  const render = hookHarness('use-app-data-operations', {
    '../app/app-utils': {},
    '../features/common/session-send-targets': {},
    './use-workspace-data-ops': { useWorkspaceDataOps: () => ({}) }
  })
  const hook = render('useAppDataOperations', {
    form,
    editingProfileId,
    desktopApi: {
      createProfile: async (payload) => {
        saved.push(payload)
        return {}
      },
      updateProfile: async (_id, payload) => {
        saved.push(payload)
        return {}
      }
    },
    setIsBusy() {},
    applySnapshot() {},
    closeConnectionForm() {},
    setFormError(error) {
      throw new Error(error)
    },
    formatError: (_scope, error) => String(error)
  })
  await hook.handleSaveProfile({ preventDefault() {} })
  assert.equal(saved.length, 1)
  return saved[0]
}

test('new connection: accept-and-save includes the tested key in the actual create payload', async () => {
  const fixture = await interactionFixture()
  await fixture.hook.acceptHostAndSave()
  assert.equal(fixture.responses[0].decision, 'accept-and-save')
  assert.equal(fixture.acceptedCount, 1)
  const saved = await saveForm(fixture.form)
  assert.equal(saved.trustedHostFingerprint, request.fingerprint)
  assert.equal(saved.host, request.host)
  assert.equal(saved.port, request.port)
})

test('existing connection: newly accepted key replaces the stale form key on update', async () => {
  const fixture = await interactionFixture({
    form: { ...draft, trustedHostFingerprint: 'SHA256:old' },
    prompt: { ...request, profileId: 'profile-1' },
    editingProfileId: 'profile-1'
  })
  await fixture.hook.acceptHostAndSave()
  assert.equal((await saveForm(fixture.form, 'profile-1')).trustedHostFingerprint, request.fingerprint)
})

for (const action of ['acceptHostOnce', 'rejectHost']) {
  test(action + ' does not save trust', async () => {
    const fixture = await interactionFixture()
    await fixture.hook[action]()
    assert.equal(fixture.acceptedCount, 0)
    assert.equal((await saveForm(fixture.form)).trustedHostFingerprint, '')
  })
}

for (const failure of [
  'SSH interaction request is no longer pending',
  'SSH interaction receiver is no longer available',
  'IPC failed'
]) {
  test('failed acknowledgement does not save trust: ' + failure, async () => {
    const fixture = await interactionFixture({ failure })
    await fixture.hook.acceptHostAndSave()
    assert.equal(fixture.acceptedCount, 0)
    assert.equal(fixture.errors.length, 1)
    assert.equal(fixture.form.trustedHostFingerprint, '')
  })
}

test('accept-and-save waits for acknowledgement and ignores duplicate clicks', async () => {
  let acknowledge
  const responseGate = new Promise((resolve) => {
    acknowledge = resolve
  })
  const fixture = await interactionFixture({ responseGate })
  const pending = fixture.hook.acceptHostAndSave()
  await fixture.hook.acceptHostAndSave()
  assert.equal(fixture.acceptedCount, 0)
  acknowledge()
  await pending
  assert.equal(fixture.acceptedCount, 1)
  assert.equal(fixture.responses.length, 1)
})

test('other sessions, profiles, jump hosts and destinations cannot write the draft key', () => {
  for (const changes of [
    { tabId: 'tab-1' },
    { profileId: 'other' },
    { authenticationTarget: 'jump-host' },
    { host: 'other.test' },
    { port: 2222 }
  ]) {
    assert.equal(acceptConnectionTestHostTrust(draft, { ...request, ...changes }, null), draft)
  }
  const ftp = { ...draft, type: 'ftp' }
  assert.equal(acceptConnectionTestHostTrust(ftp, request, null), ftp)
})

test('target-hop trust normalizes IPv6 and default ports', () => {
  const form = { ...draft, host: ' [::1] ', port: 0, jumpProfileId: 'jump-1' }
  assert.equal(
    acceptConnectionTestHostTrust(form, { ...request, host: '::1', authenticationTarget: 'target' }, null)
      .trustedHostFingerprint,
    request.fingerprint
  )
})

test('destination changes invalidate trust; labels and credentials preserve it', () => {
  const accepted = acceptConnectionTestHostTrust(draft, request, null)
  for (const changes of [{ host: 'other.test' }, { port: 2222 }, { type: 'ftp' }, { jumpProfileId: 'jump-1' }]) {
    assert.equal(invalidateChangedConnectionHostTrust(accepted, { ...accepted, ...changes }).trustedHostFingerprint, '')
  }
  for (const changes of [{ name: 'New' }, { password: 'corrected' }, { host: ' example.test ' }]) {
    const next = { ...accepted, ...changes }
    assert.equal(invalidateChangedConnectionHostTrust(accepted, next), next)
  }
})

test('form setter clears changed destinations and fresh drafts do not inherit trust', () => {
  const render = hookHarness('use-workspace-modals', {
    '../app/app-data': { defaultForm: draft },
    './connection-host-trust': { invalidateChangedConnectionHostTrust }
  })
  const options = { folders: [], profiles: [], connectionDefaults: {} }
  let modals = render('useWorkspaceModals', options)
  modals.setForm((current) => acceptConnectionTestHostTrust(current, request, null))
  modals = render('useWorkspaceModals', options)
  assert.equal(modals.form.trustedHostFingerprint, request.fingerprint)
  modals.updateForm((current) => ({ ...current, host: 'other.test' }))
  modals = render('useWorkspaceModals', options)
  assert.equal(modals.form.trustedHostFingerprint, '')
  modals.setForm(draft)
  modals.setForm((current) => acceptConnectionTestHostTrust(current, request, null))
  modals.closeConnectionForm()
  modals.openCreateModal()
  assert.equal(render('useWorkspaceModals', options).form.trustedHostFingerprint, '')
})
