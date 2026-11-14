// PostToolUse hook: run `dart format` on a .dart file Claude just wrote or edited (generated files are skipped).
// Exits 0 silently when the input isn't a .dart file or Flutter/Dart isn't installed yet.
import { spawnSync } from 'node:child_process'
import { readFileSync, existsSync } from 'node:fs'

let file = ''
try {
  const data = JSON.parse(readFileSync(0, 'utf8'))
  file = (data.tool_input && data.tool_input.file_path) || (data.tool_response && data.tool_response.filePath) || ''
} catch {
  process.exit(0)
}
if (!file.endsWith('.dart') || /\.(g|freezed|mocks)\.dart$/.test(file) || !existsSync(file)) process.exit(0)

const isWin = process.platform === 'win32'
// On Windows `dart` is dart.bat (Flutter SDK), which needs a shell. Skip paths with cmd.exe metacharacters.
if (isWin && /[&|<>^%!"]/.test(file)) process.exit(0)
spawnSync('dart', ['format', isWin ? `"${file}"` : file], { stdio: 'ignore', shell: isWin, timeout: 60_000 })
process.exit(0)
