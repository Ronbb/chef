import { readdir, readFile, mkdtemp, writeFile, rm } from 'node:fs/promises';
import { resolve, join, basename, sep } from 'node:path';
import { tmpdir } from 'node:os';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

// Immutable releases may reference revisions stored in different historic packs.
// Select the exact revision, rather than whichever filename was found last.
export async function selectSources(manifestPath, roots) {
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  const requested = new Map();
  for (const level of manifest.levels) for (const unit of level.units)
    for (const lesson of unit.lessons) {
      const key = `${lesson.lessonId}\0${lesson.revision}`;
      if (requested.has(key)) throw new Error('Duplicate release reference');
      requested.set(key, { ...lesson, levelId: level.id, unitId: unit.id });
    }
  const found = new Map();
  let count = 0;
  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      if (++count > 10000) throw new Error('Source tree exceeds limit');
      const path = join(directory, entry.name);
      if (entry.isSymbolicLink()) throw new Error('Source symlinks are not supported');
      if (entry.isDirectory()) await visit(path);
      else if (entry.isFile() && entry.name.endsWith('.lesson.json')) {
        const bytes = await readFile(path);
        if (bytes.length > 4 * 1024 * 1024) throw new Error('Lesson exceeds size limit');
        const lesson = JSON.parse(bytes.toString('utf8'));
        const key = `${lesson.id}\0${lesson.revision}`;
        const reference = requested.get(key);
        if (!reference) continue;
        if (lesson.levelId !== reference.levelId || lesson.unitId !== reference.unitId)
          throw new Error(`Release placement mismatch: ${lesson.id}`);
        if (!/^[a-z0-9][a-z0-9-]*$/.test(lesson.id)) throw new Error('Unsafe lesson ID');
        const previous = found.get(key);
        if (previous && !previous.bytes.equals(bytes))
          throw new Error(`Ambiguous immutable source: ${lesson.id} revision ${lesson.revision}`);
        if (!previous) found.set(key, { path, bytes, id: lesson.id });
      }
    }
  }
  for (const root of roots) await visit(resolve(root));
  return [...requested.keys()].map(key => {
    if (!found.has(key)) throw new Error('Missing exact release revision');
    return found.get(key);
  });
}

export async function checkCurriculum(binary, manifestPath, roots) {
  const run = args => {
    const result = spawnSync(resolve(binary), args, { stdio: 'inherit' });
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error('Chef author validation failed');
  };
  run(['check-release', resolve(manifestPath)]);
  const sources = await selectSources(manifestPath, roots);
  const directory = await mkdtemp(join(tmpdir(), 'chef-curriculum-'));
  try {
    for (const source of sources) await writeFile(join(directory, `${source.id}.lesson.json`), source.bytes);
    run(['check-release', resolve(manifestPath), '--sources', directory]);
    console.log(`Validated ${sources.length} exact revisions from course repositories; no database writes.`);
  } finally {
    const absolute = resolve(directory);
    if (!absolute.startsWith(resolve(tmpdir()) + sep) || !basename(absolute).startsWith('chef-curriculum-'))
      throw new Error('Unsafe temporary cleanup path');
    await rm(absolute, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [binary, manifest, ...roots] = process.argv.slice(2);
  if (!binary || !manifest || !roots.length) {
    console.error('Usage: node check-curriculum.mjs <chef-binary> <release.json> <course-root> [...]');
    process.exitCode = 2;
  } else {
    try { await checkCurriculum(binary, manifest, roots); }
    catch (error) { console.error(error.message); process.exitCode = 1; }
  }
}
