// The rendered App installer behaves like the CLI installer: mandatory
// checksum sidecar, verified extract, and one standalone App binary.
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import {
  chmodSync,
  existsSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync
} from 'node:fs'
import { tmpdir } from 'node:os'
import { resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import test from 'node:test'

const repoRoot = resolve(import.meta.dirname, '..')
const installerTemplate = readFileSync(
  resolve(repoRoot, 'scripts/install-app.sh'),
  'utf8'
)
const appRegistry = JSON.parse(
  readFileSync(resolve(repoRoot, 'protocol/app-registry.json'), 'utf8')
)

function renderInstaller(root, app = 'filetree') {
  const binary = `supercli-${app}`
  const rendered = installerTemplate
    .replaceAll('__DEFAULT_CHANNEL__', 'beta')
    .replaceAll('__BASE_URL__', 'https://supercli.com')
    .replaceAll('__APP__', app)
    .replaceAll('__BIN__', binary)
    .replaceAll('__TRY_LINES__', `echo "Try it:  ${binary}"`)
  assert.doesNotMatch(rendered, /__[A-Z_]+__/)
  const installer = resolve(root, 'install-app.sh')
  writeFileSync(installer, rendered)
  return installer
}

function fixture() {
  const root = mkdtempSync(resolve(tmpdir(), 'supercli-app-installer-test-'))
  const payload = resolve(root, 'payload')
  const mockBin = resolve(root, 'bin')
  const installDir = resolve(root, 'install')
  const supercliHome = resolve(root, '.supercli')
  mkdirSync(payload)
  mkdirSync(mockBin)
  const installer = renderInstaller(root)
  const binary = resolve(payload, 'supercli-filetree')
  writeFileSync(binary, [
    '#!/bin/sh',
    'echo "supercli-filetree 0.1.0"',
    ''
  ].join('\n'))
  chmodSync(binary, 0o755)

  const archive = resolve(root, 'supercli-filetree.tar.gz')
  const tar = spawnSync('tar', ['-czf', archive, '-C', payload, 'supercli-filetree'])
  assert.equal(tar.status, 0, tar.stderr?.toString())
  const digest = createHash('sha256').update(readFileSync(archive)).digest('hex')
  const sidecar = resolve(root, 'supercli-filetree.tar.gz.sha256')
  writeFileSync(sidecar, `${digest}  supercli-filetree-latest-test.tar.gz\n`)

  const curl = resolve(mockBin, 'curl')
  writeFileSync(curl, `#!/bin/sh
out=
url=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) out="$2"; shift 2 ;;
    -*) shift ;;
    *) url="$1"; shift ;;
  esac
done
case "$url" in
  *.sha256) source_file="\${MOCK_SIDECAR:?}" ;;
  *) source_file="\${MOCK_ARCHIVE:?}" ;;
esac
[ -f "$source_file" ] || exit 22
cp "$source_file" "$out"
`)
  chmodSync(curl, 0o755)

  return { root, mockBin, installDir, supercliHome, installer, archive, sidecar }
}

function runInstaller(state, sidecar = state.sidecar) {
  return spawnSync('sh', [state.installer], {
    encoding: 'utf8',
    env: {
      ...process.env,
      PATH: `${state.mockBin}:${process.env.PATH}`,
      SUPERCLI_HOME: state.supercliHome,
      SUPERCLI_CHANNEL: 'beta',
      SUPERCLI_INSTALL_BASE: 'https://release.invalid',
      SUPERCLI_INSTALL_DIR: state.installDir,
      MOCK_ARCHIVE: state.archive,
      MOCK_SIDECAR: sidecar
    }
  })
}

test('App installer requires a checksum sidecar', () => {
  const state = fixture()
  try {
    const result = runInstaller(state, resolve(state.root, 'missing.sha256'))
    assert.notEqual(result.status, 0)
    assert.match(result.stderr, /checksum sidecar is unavailable/)
    assert.equal(existsSync(resolve(state.installDir, 'supercli-filetree')), false)
  } finally {
    rmSync(state.root, { recursive: true, force: true })
  }
})

test('App installer rejects a checksum mismatch', () => {
  const state = fixture()
  const wrong = resolve(state.root, 'wrong.sha256')
  writeFileSync(wrong, `${'a'.repeat(64)}  supercli-filetree.tar.gz\n`)
  try {
    const result = runInstaller(state, wrong)
    assert.notEqual(result.status, 0)
    assert.match(result.stderr, /checksum mismatch/)
    assert.equal(existsSync(resolve(state.installDir, 'supercli-filetree')), false)
  } finally {
    rmSync(state.root, { recursive: true, force: true })
  }
})

test('App installer verifies and installs without mutating an Unpeel registry', () => {
  const state = fixture()
  try {
    const result = runInstaller(state)
    assert.equal(result.status, 0, result.stderr)
    assert.equal(existsSync(resolve(state.installDir, 'supercli-filetree')), true)
    const appHome = resolve(state.supercliHome, 'apps', 'supercli.app.filetree')
    assert.equal(existsSync(appHome), false)
    assert.match(result.stdout, /detects it automatically/)
  } finally {
    rmSync(state.root, { recursive: true, force: true })
  }
})

test('App registry covers every standalone App with a stable id', () => {
  assert.deepEqual(Object.keys(appRegistry), [
    'diffs',
    'filetree',
    'markdown',
    'usage'
  ])
  for (const [app, config] of Object.entries(appRegistry)) {
    assert.match(config.id, /^supercli\.app\.[a-z0-9-]+$/)
    assert.equal(config.binary, `supercli-${app}`)
    assert.equal(typeof config.name, 'string')
    assert.equal(typeof config.description, 'string')
    const root = mkdtempSync(resolve(tmpdir(), `supercli-${app}-installer-render-`))
    try {
      renderInstaller(root, app)
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  }
})
