#!/usr/bin/env bun
// Generate client-safe runtime metadata from each runtime.toml under runtimes/.
//
// The Rust Host consumes the descriptors directly via build.rs. This script
// emits a deterministic JSON catalog for non-Rust consumers and as a
// checked-in reference. Only declared metadata/capabilities enter this
// output; provider recipes remain Host-owned.
//
// Swift generation was removed: under the Swift-0% goal no new Swift is
// generated. The legacy generated/GeneratedRuntimeCatalog.swift reference
// copy is tracked in the port map as a dropped file.

import { lstat, readdir, realpath } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptPath = fileURLToPath(import.meta.url)
const repoRoot = path.dirname(path.dirname(scriptPath))
const runtimesRoot = path.join(repoRoot, 'runtimes')
// The generated JSON catalog is checked in as the reference copy;
// `--check` verifies it matches runtimes/. Use `--out <path>` to write
// elsewhere.
const cliArgs = process.argv.slice(2)
const outIndex = cliArgs.indexOf('--out')
const jsonOutput = outIndex >= 0 && cliArgs[outIndex + 1]
  ? path.resolve(cliArgs[outIndex + 1])
  : path.join(
      repoRoot,
      'generated/runtime-catalog.json'
    )
const colorPattern = /^#[0-9A-Fa-f]{6}$/
const maxRuntimeIconBytes = 128 * 1024

async function loadIconSVG(display, source) {
  const relativePath = display.icon_asset
  const iconSource = display.icon_source
  const iconLicense = display.icon_license
  if (relativePath == null) {
    if (iconSource != null || iconLicense != null) {
      throw new Error(
        `${path.relative(repoRoot, source)}: icon_source/icon_license require display.icon_asset`
      )
    }
    if (display.icon_template === false) {
      throw new Error(
        `${path.relative(repoRoot, source)}: display.icon_template=false requires icon_asset`
      )
    }
    return null
  }

  if (typeof relativePath !== 'string') {
    throw new Error(`${path.relative(repoRoot, source)}: display.icon_asset must be a string`)
  }
  const segments = relativePath.split('/')
  if (
    path.isAbsolute(relativePath) ||
    relativePath.includes('\\') ||
    segments[0] !== 'assets' ||
    segments.some((segment) => segment === '' || segment === '.' || segment === '..') ||
    path.posix.extname(relativePath) !== '.svg'
  ) {
    throw new Error(
      `${path.relative(repoRoot, source)}: display.icon_asset must be a safe SVG path below assets/`
    )
  }
  if (
    typeof iconSource !== 'string' ||
    !(iconSource.startsWith('https://') || iconSource.startsWith('internal:')) ||
    /\s/.test(iconSource) ||
    typeof iconLicense !== 'string' ||
    iconLicense.trim() === '' ||
    iconLicense.trim() !== iconLicense
  ) {
    throw new Error(
      `${path.relative(repoRoot, source)}: icon assets require valid display.icon_source and display.icon_license`
    )
  }

  const assetPath = path.join(path.dirname(source), relativePath)
  const asset = Bun.file(assetPath)
  if (!(await asset.exists())) {
    throw new Error(
      `${path.relative(repoRoot, source)}: missing ${path.relative(repoRoot, assetPath)}`
    )
  }
  const assetMetadata = await lstat(assetPath)
  if (assetMetadata.isSymbolicLink() || !assetMetadata.isFile()) {
    throw new Error(`${path.relative(repoRoot, assetPath)} must be a regular file, not a symlink`)
  }
  const [runtimeRoot, resolvedAssetPath] = await Promise.all([
    realpath(path.dirname(source)),
    realpath(assetPath)
  ])
  const resolvedRelativePath = path.relative(runtimeRoot, resolvedAssetPath)
  if (
    path.isAbsolute(resolvedRelativePath) ||
    resolvedRelativePath === '..' ||
    resolvedRelativePath.startsWith(`..${path.sep}`)
  ) {
    throw new Error(`${path.relative(repoRoot, assetPath)} escapes its runtime package`)
  }
  if (assetMetadata.size > maxRuntimeIconBytes) {
    throw new Error(
      `${path.relative(repoRoot, assetPath)} exceeds the ${maxRuntimeIconBytes} byte icon limit`
    )
  }
  const svg = (await asset.text()).trim()
  if (svg === '' || !svg.includes('<svg') || !svg.includes('</svg>')) {
    throw new Error(`${path.relative(repoRoot, assetPath)} is not a complete UTF-8 SVG document`)
  }
  return svg
}

const capabilityCases = new Map([
  ['lifecycle_hooks', 'lifecycleHooks'],
  ['resume', 'resume'],
  ['restart_agent', 'restartAgent'],
  ['mcp_sessions', 'mcpSessions'],
  ['mcp_browser', 'mcpBrowser'],
  ['mcp_computer', 'mcpComputer'],
  ['transcript', 'transcript'],
  ['notify_when_done', 'notifyWhenDone'],
  ['semantic_terminal_title', 'semanticTerminalTitle']
])

const platformCases = new Map([
  ['macos', 'macos'],
  ['linux', 'linux']
])

async function loadDescriptors() {
  const directoryEntries = await readdir(runtimesRoot, { withFileTypes: true })
  const descriptors = []
  for (const entry of directoryEntries) {
    if (!entry.isDirectory()) continue
    const source = path.join(runtimesRoot, entry.name, 'runtime.toml')
    const file = Bun.file(source)
    if (!(await file.exists())) continue
    let descriptor
    try {
      descriptor = Bun.TOML.parse(await file.text())
    } catch (error) {
      throw new Error(`${path.relative(repoRoot, source)}: ${error.message}`)
    }
    if (descriptor.slug !== entry.name) {
      throw new Error(
        `${path.relative(repoRoot, source)}: slug ${JSON.stringify(descriptor.slug)} ` +
          `must match directory ${JSON.stringify(entry.name)}`
      )
    }
    const iconSVG = await loadIconSVG(descriptor.display, source)
    descriptors.push({ source, descriptor, iconSVG })
  }
  if (descriptors.length === 0) {
    throw new Error(`${path.relative(repoRoot, runtimesRoot)}: no runtime.toml descriptors found`)
  }
  descriptors.sort((left, right) => {
    const leftOrder = left.descriptor.legacy_order ?? Number.MAX_SAFE_INTEGER
    const rightOrder = right.descriptor.legacy_order ?? Number.MAX_SAFE_INTEGER
    return (
      leftOrder - rightOrder ||
      left.descriptor.slug.localeCompare(right.descriptor.slug) ||
      left.descriptor.id.localeCompare(right.descriptor.id)
    )
  })
  return descriptors
}

function renderJson(descriptors) {
  const runtimes = descriptors.map(({ descriptor, iconSVG }) => ({
    ...descriptor,
    icon_svg: iconSVG ?? null,
  }))
  const catalog = {
    generated_by: 'scripts/generate-runtime-client-catalog.mjs',
    source_of_truth: 'runtimes/*/runtime.toml — do not edit by hand',
    runtime_count: runtimes.length,
    runtimes,
  }
  return JSON.stringify(catalog, null, 2) + '\n'
}

async function main() {
  const check = cliArgs.includes('--check')
  const rendered = renderJson(await loadDescriptors())
  const output = Bun.file(jsonOutput)
  if (check) {
    const current = (await output.exists()) ? await output.text() : ''
    if (current !== rendered) {
      console.error(
        `${path.relative(repoRoot, jsonOutput)} is stale; ` +
          'run bun scripts/generate-runtime-client-catalog.mjs'
      )
      process.exitCode = 1
    }
    return
  }
  await Bun.write(jsonOutput, rendered)
}

await main().catch((error) => {
  console.error(`runtime client catalog generation failed: ${error.message}`)
  process.exitCode = 1
})
