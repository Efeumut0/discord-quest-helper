import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, rmSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { nextVersion, releaseDate, updateManifest } from '../scripts/release.mjs';

test('release date follows Istanbul time rather than the runner timezone', () => {
    assert.equal(releaseDate(new Date('2026-11-25T22:15:00Z')), '2026-11-26');
    assert.equal(nextVersion('2026-11-26', []), '2026.11.2601');
});

test('release preparation synchronizes app, runner, installer and Windows metadata', () => {
    const temporary = mkdtempSync(join(tmpdir(), 'dqh-version-test-'));
    try {
        const project = join(temporary, 'project');
        const exporter = fileURLToPath(new URL('../scripts/export-source.mjs', import.meta.url));
        const exported = spawnSync(process.execPath, [exporter, project], { encoding: 'utf8' });
        assert.equal(exported.status, 0, exported.stderr);
        const releases = join(temporary, 'releases.json');
        const tags = join(temporary, 'tags.json');
        const info = join(temporary, 'info.json');
        writeFileSync(releases, '[]');
        writeFileSync(tags, JSON.stringify([{ name: 'v2026.11.2601' }]));
        const result = spawnSync(process.execPath, [join(project, 'scripts/release.mjs'), 'prepare',
            '--date', '2026-11-26', '--releases', releases, '--tags', tags, '--info', info], { encoding: 'utf8' });
        assert.equal(result.status, 0, result.stderr);
        assert.equal(JSON.parse(readFileSync(info)).version, '2026.11.2602');
        assert.equal(JSON.parse(readFileSync(join(project, 'package.json'))).version, '2026.11.2602');
        assert.equal(JSON.parse(readFileSync(join(project, 'src-tauri/tauri.conf.json'))).version, '2026.11.2602');
        for (const directory of ['src-tauri', 'src-win']) {
            assert.match(readFileSync(join(project, directory, 'Cargo.toml'), 'utf8'), /^version = "2026\.11\.2602"$/m);
            const name = directory === 'src-tauri' ? 'discord-quest-helper' : 'src-win';
            assert.match(readFileSync(join(project, directory, 'Cargo.lock'), 'utf8'),
                new RegExp(`name = "${name}"\\r?\\nversion = "2026\\.11\\.2602"`));
        }
        assert.match(readFileSync(join(project, 'src-win/embed_icon.rc'), 'utf8'), /FILEVERSION 2026,11,2602,0/);
    } finally {
        rmSync(temporary, { recursive: true, force: true });
    }
});

test('same-day releases increment without confusing old or future releases', () => {
    const releases = ['v25.11.0', 'v2026.11.2509', 'v2026.11.2601', 'v2026.11.2603'];
    assert.equal(nextVersion('2026-11-26', releases), '2026.11.2604');
    assert.equal(nextVersion('2026-12-01', releases), '2026.12.101');
    assert.throws(() => nextVersion('2026-02-30', []));
    assert.throws(() => nextVersion('2026-11-26', ['v2026.11.2699']));
});

test('updater metadata points to the matching signed setup asset', () => {
    const metadata = updateManifest('2026.11.2601', 'signed-content', '2026-11-26T10:00:00Z');
    assert.equal(metadata.version, '2026.11.2601');
    assert.equal(metadata.platforms['windows-x86_64'].signature, 'signed-content');
    assert.equal(metadata.platforms['windows-x86_64'].url,
        'https://github.com/Efeumut0/discord-quest-helper/releases/download/v2026.11.2601/DiscordQuestHelper-2026.11.2601-windows-x64-setup.exe');
    assert.throws(() => updateManifest('2026.11.2601', '', '2026-11-26T10:00:00Z'));
});
