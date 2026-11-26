import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const exporter = fileURLToPath(new URL('../scripts/export-source.mjs', import.meta.url));

function exportSource(script, destination) {
    const result = spawnSync(process.execPath, [script, destination], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr || result.stdout);
}

test('source export keeps code and excludes working notes and office files', () => {
    const temporary = mkdtempSync(join(tmpdir(), 'dqh-export-test-'));
    try {
        const fixture = join(temporary, 'project');
        exportSource(exporter, fixture);
        const excluded = [
            'src/WORKING_NOTES.md',
            'src/PROJECT_HANDOFF.md',
            'src/INTERNAL.md',
            'src/discord-api-research-2026-11-26.md',
            'src/discord-api-evidence-2026-11-26.json',
            'src/imported-data.xlsx',
            'src/imported-data.csv',
            'src/debug.log',
        ];
        for (const file of excluded) writeFileSync(join(fixture, file), 'Internal working material');
        writeFileSync(join(fixture, 'src/export-fixture.ts'), 'export const fixture = true;\n');

        const destination = join(temporary, 'published');
        exportSource(join(fixture, 'scripts/export-source.mjs'), destination);
        for (const file of excluded) assert.equal(existsSync(join(destination, file)), false, file);
        assert.equal(existsSync(join(destination, 'src/export-fixture.ts')), true);
        assert.equal(existsSync(join(destination, 'README.md')), true);
        assert.equal(existsSync(join(destination, 'src-tauri/resources/.gitkeep')), true);
    } finally {
        rmSync(temporary, { recursive: true, force: true });
    }
});
