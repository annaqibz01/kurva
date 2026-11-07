import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const OUTPUT_DIR = 'contexts';
const CACHE_FILE = path.join(OUTPUT_DIR, '.cache.json');
const TREE_FILE = 'directory-tree.txt';
const CONTRACTS_FILE = path.join(OUTPUT_DIR, 'architecture-contracts.txt');

// Dokumen kontrak utama di akar repo yang wajib masuk paling awal ke bundle arsitektur.
// AI agent membaca dokumen aturan rekayasa, arsitektur pilar, dan konfigurasi compiler/workspace lebih dulu.
const ROOT_CONTRACT_DOCS = [
  'AGENT_RULES.md',
  'ARCHITECTURE.md',
  'Cargo.toml',
  '.cargo/config.toml',
];

const IGNORED_DIRS = new Set([
  'target', // Build output Rust (sangat besar, wajib diabaikan)
  '.git',
  '.vscode',
  'node_modules',
  'dist',
  'build',
  'coverage',
  'tmp',
  OUTPUT_DIR,
]);

const IGNORED_FILES = new Set([
  'cargo.lock', // Menghindari konsumsi token raksasa dari lockfile dependensi
  'directory-tree.txt',
  'generate-context.js',
  'generate-tree.js',
  '.cache.json',
  '.ds_store',
  'thumbs.db',
]);

const VALID_EXTENSIONS = new Set([
  '.rs',
  '.toml',
  '.md',
  '.json',
  '.sql',
  '.sh',
  '.ps1',
]);

const LANG_MAP = {
  '.rs': 'rust',
  '.toml': 'toml',
  '.md': 'markdown',
  '.json': 'json',
  '.sql': 'sql',
  '.sh': 'bash',
  '.ps1': 'powershell',
};

if (!fs.existsSync(OUTPUT_DIR)) {
  fs.mkdirSync(OUTPUT_DIR, { recursive: true });
}

function compressCode(content) {
  return content
    .replace(/\r\n/g, '\n')
    .replace(/\/\/\s*[-=*#]{3,}\s*$/gm, '')
    .replace(/\/\*\s*[-=*#]{3,}\s*\*\/$/gm, '')
    .split('\n')
    .map((line) => line.trimEnd())
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

function getProjectName(rootDir) {
  try {
    const cargoPath = path.join(rootDir, 'Cargo.toml');
    if (fs.existsSync(cargoPath)) {
      const content = fs.readFileSync(cargoPath, 'utf8');
      const pkgMatch = content.match(/\[package\][\s\S]*?name\s*=\s*["']([^"']+)["']/);
      if (pkgMatch) return pkgMatch[1];
      if (content.includes('[workspace]')) return 'kurva-workspace';
    }
  } catch {
    // Fallback
  }
  return 'kurva';
}

function generateCleanTree(dir, prefix = '') {
  let tree = '';
  const entries = fs
    .readdirSync(dir)
    .filter((item) => !IGNORED_DIRS.has(item) && !IGNORED_FILES.has(item.toLowerCase()))
    .sort((a, b) => {
      const aPath = path.join(dir, a);
      const bPath = path.join(dir, b);
      const aIsDir = fs.statSync(aPath).isDirectory();
      const bIsDir = fs.statSync(bPath).isDirectory();
      if (aIsDir && !bIsDir) return -1;
      if (!aIsDir && bIsDir) return 1;
      return a.localeCompare(b);
    });

  entries.forEach((entry, index) => {
    const isLast = index === entries.length - 1;
    const fullPath = path.join(dir, entry);
    const stats = fs.statSync(fullPath);

    tree += `${prefix}${isLast ? '└── ' : '├── '}${entry}\n`;

    if (stats.isDirectory()) {
      tree += generateCleanTree(fullPath, prefix + (isLast ? '    ' : '│   '));
    }
  });

  return tree;
}

export function runGenerateTree() {
  try {
    const rootDir = process.cwd();
    const projectName = getProjectName(rootDir);
    const output = `Proyek: ${projectName}\nRoot: ${rootDir}\nTanggal Update: ${new Date().toLocaleString('id-ID')}\n\n${generateCleanTree(rootDir)}`;
    fs.writeFileSync(TREE_FILE, output, 'utf8');
    console.log(`[Tree Updated] ${TREE_FILE} berhasil diperbarui (bersih tanpa direktori target/).`);
  } catch (err) {
    console.warn('[Tree Error] Gagal membuat directory tree:', err.message);
  }
}

function walkDir(dir, fileList = []) {
  const items = fs.readdirSync(dir);
  for (const item of items) {
    const fullPath = path.join(dir, item);
    const stat = fs.statSync(fullPath);

    if (stat.isDirectory()) {
      if (!IGNORED_DIRS.has(item)) {
        walkDir(fullPath, fileList);
      }
    } else {
      const lower = item.toLowerCase();
      if (!IGNORED_FILES.has(lower) && VALID_EXTENSIONS.has(path.extname(item).toLowerCase())) {
        fileList.push({ fullPath, mtime: stat.mtimeMs });
      }
    }
  }
  return fileList;
}

/**
 * Pengelompokan modular berbasis crate dan peran workspace kurva:
 * Mendukung struktur `crates/<crate-name>` maupun flat root `<crate-name>`.
 */
function determineGroup(relPath) {
  const normalized = relPath.replace(/\\/g, '/');

  // Struktur monorepo crates/
  if (normalized.startsWith('crates/')) {
    const parts = normalized.split('/');
    if (parts.length > 1) {
      return `crate-${parts[1]}`;
    }
  }

  // Struktur crate kurva langsung di root
  const rootCrateMatch = normalized.match(/^(kurva-[a-z0-9_-]+)\//);
  if (rootCrateMatch) {
    return `crate-${rootCrateMatch[1]}`;
  }

  if (normalized.startsWith('benches/')) {
    return 'benchmarks';
  }
  if (normalized.startsWith('tests/')) {
    return 'integration-tests';
  }
  if (normalized.startsWith('scripts/')) {
    return 'tooling-scripts';
  }
  if (normalized.startsWith('docs/')) {
    return 'documentation';
  }

  return 'workspace-root';
}

/**
 * Generator Arsitektur Khusus AI (kurva Engine):
 * Mengumpulkan:
 * 1. Root docs rekayasa (AGENT_RULES.md, ARCHITECTURE.md, Cargo.toml root).
 * 2. Seluruh Cargo.toml milik masing-masing crate (peta batas crate & dependensi).
 * 3. File lib.rs & main.rs (titik ekspor API publik tiap crate).
 * 4. Berkas definisi types, traits, and error contracts.
 */
export function runArchitectureContracts() {
  const rootDir = process.cwd();
  const allFiles = walkDir(rootDir);
  const entries = [];

  for (const { fullPath } of allFiles) {
    const relPath = path.relative(rootDir, fullPath).replace(/\\/g, '/');

    const isRootContract = ROOT_CONTRACT_DOCS.includes(relPath);
    const isCrateManifest = relPath.endsWith('Cargo.toml');
    const isPublicEntry = relPath.endsWith('/src/lib.rs') || relPath.endsWith('/src/main.rs');
    const isContractTypeOrTrait =
      relPath.endsWith('/types.rs') ||
      relPath.endsWith('/traits.rs') ||
      relPath.endsWith('/contract.rs') ||
      relPath.endsWith('/error.rs') ||
      relPath.includes('/types/') ||
      relPath.includes('/traits/');

    if (isRootContract || isCrateManifest || isPublicEntry || isContractTypeOrTrait) {
      const raw = fs.readFileSync(fullPath, 'utf8');
      const clean = compressCode(raw);
      const ext = path.extname(relPath).toLowerCase();
      entries.push({
        relPath,
        label: `\`${relPath}\``,
        text: `### \`${relPath}\`\n\`\`\`${LANG_MAP[ext] || 'text'}\n${clean}\n\`\`\``,
      });
    }
  }

  // Prioritaskan dokumen kontrak utama di awal file, sisanya urut alfabetis
  const ordered = entries.sort((a, b) => {
    const ai = ROOT_CONTRACT_DOCS.indexOf(a.relPath);
    const bi = ROOT_CONTRACT_DOCS.indexOf(b.relPath);
    if (ai !== -1 || bi !== -1) {
      if (ai === -1) return 1;
      if (bi === -1) return -1;
      return ai - bi;
    }
    return a.relPath.localeCompare(b.relPath);
  });

  const manifest = ordered.map((entry) => `- ${entry.label}`);
  const sections = ordered.map((entry) => entry.text);

  const header = `# CONTEXT: KURVA ARCHITECTURE CONTRACTS (CRATE BOUNDARIES, TRAITS & TYPES)\n\n## Included Contracts:\n${manifest.join('\n')}\n\n---\n\n`;
  fs.writeFileSync(CONTRACTS_FILE, header + sections.join('\n\n'), 'utf8');
  console.log(`[Contracts Generated] ${CONTRACTS_FILE} berhasil disusun (${ordered.length} kontrak).`);
}

export function runIncrementalContext(targetGroupFilter = null) {
  const rootDir = process.cwd();
  const allFiles = walkDir(rootDir);

  let cache = {};
  if (fs.existsSync(CACHE_FILE)) {
    try {
      cache = JSON.parse(fs.readFileSync(CACHE_FILE, 'utf8'));
    } catch {
      cache = {};
    }
  }

  const groups = new Map();
  const dirtyGroups = new Set();
  const currentFileSet = new Set();

  for (const { fullPath, mtime } of allFiles) {
    const relPath = path.relative(rootDir, fullPath).replace(/\\/g, '/');
    currentFileSet.add(relPath);

    const groupKey = determineGroup(relPath);
    if (!groups.has(groupKey)) {
      groups.set(groupKey, []);
    }
    groups.get(groupKey).push({ fullPath, relPath, mtime });

    if (!cache[relPath] || cache[relPath].mtime !== mtime) {
      dirtyGroups.add(groupKey);
    }
  }

  for (const cachedRelPath of Object.keys(cache)) {
    if (!currentFileSet.has(cachedRelPath)) {
      const g = determineGroup(cachedRelPath);
      dirtyGroups.add(g);
      delete cache[cachedRelPath];
    }
  }

  const groupsToProcess = targetGroupFilter ? [targetGroupFilter] : Array.from(dirtyGroups);

  if (groupsToProcess.length === 0) {
    console.log('[Context] Seluruh modul context up-to-date.');
    return;
  }

  let updatedCount = 0;
  for (const groupKey of groupsToProcess) {
    const fileEntries = groups.get(groupKey) || [];
    fileEntries.sort((a, b) => a.relPath.localeCompare(b.relPath));

    const manifest = [];
    const filesContent = [];

    for (const entry of fileEntries) {
      const ext = path.extname(entry.fullPath).toLowerCase();
      const lang = LANG_MAP[ext] || '';

      try {
        const raw = fs.readFileSync(entry.fullPath, 'utf8');
        const clean = compressCode(raw);
        manifest.push(`- \`${entry.relPath}\``);
        filesContent.push(`### \`${entry.relPath}\`\n\`\`\`${lang}\n${clean}\n\`\`\``);

        cache[entry.relPath] = { mtime: entry.mtime };
      } catch (err) {
        manifest.push(`- \`${entry.relPath}\` (Gagal dimuat)`);
        filesContent.push(`### \`${entry.relPath}\`\n> [Error: ${err.message}]`);
      }
    }

    const outName = `${groupKey}.txt`;
    const outPath = path.join(OUTPUT_DIR, outName);

    if (fileEntries.length > 0) {
      const header = `# CONTEXT: ${groupKey.toUpperCase()}\n\n## Included Files:\n${manifest.join('\n')}\n\n---\n\n`;
      fs.writeFileSync(outPath, header + filesContent.join('\n\n'), 'utf8');
      console.log(`[Context Updated] ${OUTPUT_DIR}/${outName}`);
      updatedCount++;
    } else if (fs.existsSync(outPath)) {
      fs.unlinkSync(outPath);
    }
  }

  fs.writeFileSync(CACHE_FILE, JSON.stringify(cache, null, 2), 'utf8');

  if (updatedCount > 0) {
    console.log(`[Context] ${updatedCount} grup context diperbarui secara inkremental.`);
  }
}

// CLI Dispatcher
const isDirectRun = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (isDirectRun) {
  const args = process.argv.slice(2);

  if (args.includes('--tree')) {
    runGenerateTree();
  } else if (args.includes('--contracts')) {
    runArchitectureContracts();
  } else if (args.includes('--group')) {
    const groupIdx = args.indexOf('--group') + 1;
    const targetGroup = args[groupIdx];
    if (targetGroup) {
      runIncrementalContext(targetGroup);
    } else {
      console.warn('[Context Warning] Masukkan nama group target setelah --group.');
    }
  } else {
    runGenerateTree();
    runArchitectureContracts();
    runIncrementalContext();
  }
}