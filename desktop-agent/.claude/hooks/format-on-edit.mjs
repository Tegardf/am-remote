// PostToolUse hook: run rustfmt on a .rs file Claude just wrote or edited.
// Exits 0 silently when the input isn't a .rs file or rustfmt isn't installed yet.
import { spawnSync } from 'node:child_process'
import { readFileSync, existsSync } from 'node:fs'

let file = ''
try {
  const data = JSON.parse(readFileSync(0, 'utf8'))
  file = (data.tool_input && data.tool_input.file_path) || (data.tool_response && data.tool_response.filePath) || ''
} catch {
  process.exit(0)
}
if (!file.endsWith('.rs') || !existsSync(file)) process.exit(0)

// rustfmt is a real .exe on Windows, so no shell is needed and the path is passed as-is.
spawnSync('rustfmt', ['--edition', '2021', file], { stdio: 'ignore', timeout: 30_000 })
process.exit(0)
