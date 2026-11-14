#!/usr/bin/env node
// Validates the Markdown-based Claude setup of this repo:
//   - contract/samples.md: every "### key" has exactly one ```json block that parses; keys and fields are snake_case
//   - */.claude/tasks/mvp-*.md: required sections present and the file points at _workflow-rules.md
//   - every .claude/settings.json parses
//
//   node .claude/scripts/check-setup.mjs            check everything (exit 1 on problems)
//   node .claude/scripts/check-setup.mjs <file...>  check the given files
//   node .claude/scripts/check-setup.mjs --hook     PostToolUse hook: check only the edited file, exit 2 + stderr on problems
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..')
const PROJECTS = ['desktop-agent', 'android-controller']
const SAMPLES = 'contract/samples.md'
const MVP_SECTIONS = ['## Goal', '## Gate', '## Build', '## Tests (required)', '## Done when', '## Smoke test', '## Next']
const SNAKE = /^[a-z][a-z0-9_]*$/

const rel = (f) => path.relative(ROOT, path.resolve(ROOT, f)).split(path.sep).join('/')
const read = (r) => fs.readFileSync(path.join(ROOT, r), 'utf8').replace(/\r\n/g, '\n')

// Parse "### key\n```json\n...\n```" blocks. Exported shape matches what the projects' tests must do.
export function parseSamples(md) {
  const out = {}
  const errors = []
  const parts = md.split(/^### /m).slice(1)
  for (const part of parts) {
    const key = part.slice(0, part.indexOf('\n')).trim()
    const blocks = [...part.matchAll(/```json\n([\s\S]*?)\n```/g)]
    if (!SNAKE.test(key)) errors.push(`sample key "${key}" must be snake_case`)
    if (blocks.length !== 1) { errors.push(`"${key}" needs exactly one json block (found ${blocks.length})`); continue }
    if (key in out) errors.push(`duplicate sample key "${key}"`)
    try { out[key] = JSON.parse(blocks[0][1]) } catch (e) { errors.push(`"${key}": invalid JSON: ${e.message}`) }
  }
  if (!parts.length) errors.push('no "### key" samples found')
  return { samples: out, errors }
}

function checkSamples(r) {
  const { samples, errors } = parseSamples(read(r))
  const walk = (v, at) => {
    if (Array.isArray(v)) v.forEach((x, i) => walk(x, `${at}[${i}]`))
    else if (v && typeof v === 'object')
      for (const [k, x] of Object.entries(v)) {
        if (!SNAKE.test(k)) errors.push(`${at}.${k}: wire field names must be snake_case`)
        walk(x, `${at}.${k}`)
      }
  }
  for (const [k, v] of Object.entries(samples)) walk(v, k)
  return errors
}

function checkMvpTask(r) {
  const src = read(r)
  const errors = MVP_SECTIONS.filter((s) => !src.includes(`\n${s}`)).map((s) => `missing section "${s}"`)
  if (!src.includes('_workflow-rules.md')) errors.push('must point at _workflow-rules.md')
  return errors
}

function checkJson(r) {
  try { JSON.parse(read(r)); return [] } catch (e) { return ['invalid JSON: ' + e.message] }
}

function checkFile(f) {
  const r = rel(f)
  if (r === SAMPLES) return checkSamples(r)
  if (/(^|\/)\.claude\/tasks\/mvp-\d+\.md$/.test(r)) return checkMvpTask(r)
  if (/(^|\/)\.claude\/settings(\.local)?\.json$/.test(r)) return checkJson(r)
  return null
}

function allFiles() {
  const files = [SAMPLES, '.claude/settings.json']
  for (const p of PROJECTS) {
    files.push(`${p}/.claude/settings.json`)
    const dir = path.join(ROOT, p, '.claude', 'tasks')
    if (fs.existsSync(dir)) for (const f of fs.readdirSync(dir)) if (/^mvp-\d+\.md$/.test(f)) files.push(`${p}/.claude/tasks/${f}`)
  }
  return files.filter((f) => fs.existsSync(path.join(ROOT, f)))
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.includes('--hook')) {
    let file = ''
    try {
      const data = JSON.parse(fs.readFileSync(0, 'utf8'))
      file = (data.tool_input && data.tool_input.file_path) || (data.tool_response && data.tool_response.filePath) || ''
    } catch { process.exit(0) }
    if (!file || !fs.existsSync(file)) process.exit(0)
    const errors = checkFile(file)
    if (errors && errors.length) {
      process.stderr.write(`check-setup: ${rel(file)}\n${errors.map((e) => '  - ' + e).join('\n')}\n`)
      process.exit(2)
    }
    process.exit(0)
  }
  const targets = process.argv.slice(2).filter((a) => !a.startsWith('--'))
  let failed = 0
  for (const f of targets.length ? targets : allFiles()) {
    const errors = checkFile(f)
    if (errors === null) { console.log(`skip  ${rel(f)}`); continue }
    if (errors.length) { failed++; console.log(`FAIL  ${rel(f)}\n${errors.map((e) => '      - ' + e).join('\n')}`) }
    else console.log(`ok    ${rel(f)}`)
  }
  process.exit(failed ? 1 : 0)
}
