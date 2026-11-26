import { mkdir, readdir, readFile, writeFile, copyFile, stat } from 'node:fs/promises';
import { resolve, dirname, relative, extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
if (!process.argv[2]) throw new Error('Usage: node scripts/export-source.mjs <new-source-directory>');
const destination = resolve(process.argv[2]);
try {
  await stat(destination);
  throw new Error(`Destination already exists; choose a new empty destination: ${destination}`);
} catch (error) {
  if (error.code !== 'ENOENT') throw error;
}
const directories = ['src', 'src-tauri/src', 'src-tauri/examples', 'src-tauri/icons', 'src-tauri/capabilities', 'src-win/src', 'src-win/tests', 'public', 'scripts', '.github', 'tests'];
const files = ['LICENSE', 'README.md', 'README.tr.md', 'CONTRIBUTING.md', 'SECURITY.md', '.gitattributes', '.gitignore', 'package.json', 'pnpm-lock.yaml', 'index.html', 'tsconfig.json', 'tsconfig.node.json', 'vite.config.ts', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'src-tauri/build.rs', 'src-tauri/tauri.conf.json', 'src-tauri/.gitignore', 'src-win/Cargo.toml', 'src-win/Cargo.lock', 'src-win/build.rs', 'src-win/embed_icon.rc', 'src-win/icon.ico', 'src-win/README.md', 'src-win/.gitignore'];
const retired = new Set(['seal.rs', 'license_admin.rs', 'main_with_tray_icon.rs', 'tauri.svg', 'vite.svg', 'vue.svg']);
const blockedDirs = new Set(['node_modules', 'target', 'dist', '.git', 'secrets', 'data', 'metadata', 'games', 'gen', 'license-runtime', 'seal-core']);
const textExtensions = new Set(['.rs', '.ts', '.vue', '.js', '.mjs', '.json', '.yaml', '.yml', '.toml', '.md', '.ps1', '.html', '.css', '.rc', '.lock']);
const allowedExtensions = new Set([...textExtensions, '.svg', '.png', '.ico', '.icns']);
const workingNoteName = /(?:^|[-_.])(?:handoff|notes|research|evidence)(?:[-_.]|$)/i;
// Inside source directories only README files are documentation; every other
// markdown file is working material and stays local.
const allowedMarkdown = /^README(?:\.[a-z]{2})?\.md$/i;
const secretPattern = /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|(?:ghp_|github_pat_)[A-Za-z0-9_]{20,}|\b(?:WORKER_SIGNING_PRIVATE_KEY|LICENSE_CREDENTIAL_ENCRYPTION_KEY|ADMIN_BOOTSTRAP_TOKEN)\s*[:=]/;

async function collect(directory) {
  for (const entry of await readdir(join(root, directory), { withFileTypes: true })) {
    if (entry.isSymbolicLink()) throw new Error(`Symlinks are not exported: ${directory}/${entry.name}`);
    if (retired.has(entry.name) || blockedDirs.has(entry.name) || workingNoteName.test(entry.name)) continue;
    if (entry.isDirectory() && entry.name.startsWith('.')) continue;
    if (!entry.isDirectory() && extname(entry.name).toLowerCase() === '.md'
        && !directory.startsWith('.github') && !allowedMarkdown.test(entry.name)) continue;
    const item = `${directory}/${entry.name}`;
    if (entry.isDirectory()) await collect(item);
    else if (allowedExtensions.has(extname(entry.name).toLowerCase()) && !entry.name.startsWith('.env')) files.push(item);
  }
}
for (const directory of directories) await collect(directory);
for (const file of files) {
  const from = resolve(root, file);
  if (relative(root, from).startsWith('..')) throw new Error('Export escaped source root');
  if (textExtensions.has(extname(file))) {
    const content = await readFile(from, 'utf8');
    if (secretPattern.test(content)) throw new Error(`Potential secret detected; review before publication: ${file}`);
  }
}
for (const file of files) {
  const to = join(destination, file);
  await mkdir(dirname(to), { recursive: true });
  await copyFile(join(root, file), to);
}
// Tauri embeds this runner after the build script compiles it.
await mkdir(join(destination, 'src-tauri', 'resources'), { recursive: true });
await writeFile(join(destination, 'src-tauri', 'resources', '.gitkeep'), '');
console.log(`Exported ${files.length} source files to ${destination}. Compiled binaries and local working files are excluded.`);
