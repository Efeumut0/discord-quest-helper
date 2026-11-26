import { readFile, writeFile, readdir, appendFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const repository = 'https://github.com/Efeumut0/discord-quest-helper';

export function releaseDate(now = new Date()) {
    const parts = new Intl.DateTimeFormat('en-GB', {
        timeZone: 'Europe/Istanbul', year: 'numeric', month: '2-digit', day: '2-digit',
    }).formatToParts(now);
    const value = type => parts.find(part => part.type === type).value;
    return `${value('year')}-${value('month')}-${value('day')}`;
}

function calendarParts(date) {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) throw new Error('Date must be YYYY-MM-DD');
    const parsed = new Date(`${date}T00:00:00Z`);
    if (!Number.isFinite(parsed.getTime()) || parsed.toISOString().slice(0, 10) !== date) {
        throw new Error('Invalid release date');
    }
    const [year, month, day] = date.split('-').map(Number);
    if (year < 2026 || year > 9999) throw new Error('Unsupported release year');
    return { year, month, day };
}

export function nextVersion(date, existingTags) {
    const { year, month, day } = calendarParts(date);
    let sequence = 0;
    for (const tag of existingTags) {
        const match = String(tag).match(/^v?(\d+)\.(\d+)\.(\d+)$/);
        if (!match) continue;
        const [, major, minor, patch] = match.map(Number);
        if (major === year && minor === month && Math.floor(patch / 100) === day) {
            sequence = Math.max(sequence, patch % 100);
        }
    }
    if (sequence >= 99) throw new Error('The daily release limit has been reached');
    return `${year}.${month}.${day * 100 + sequence + 1}`;
}

export function setupName(version) {
    if (!/^\d{4}\.\d{1,2}\.\d{3,4}$/.test(version)) throw new Error('Invalid release version');
    return `DiscordQuestHelper-${version}-windows-x64-setup.exe`;
}

export function updateManifest(version, signature, publishedAt = new Date().toISOString()) {
    if (!signature.trim()) throw new Error('A signed installer is required');
    return {
        version,
        notes: 'Setup and automatic update. Kurulum ve otomatik güncelleme.',
        pub_date: publishedAt,
        platforms: {
            'windows-x86_64': {
                signature: signature.trim(),
                url: `${repository}/releases/download/v${version}/${setupName(version)}`,
            },
        },
    };
}

function notes(version, date) {
    const display = date.split('-').reverse().join('.');
    return `## Discord Quest Helper — ${display}\n\nVersion: ${version}\n\n` +
        '### Install and use\n\n' +
        `Download \`${setupName(version)}\`, run it and open Discord Quest Helper from the Start menu. Setup installs for the current Windows user and installs WebView2 if needed.\n\n` +
        'Keep Discord open, accept the play-time quest, search for its game and use Source 1 when available. Create and start the helper, then check progress on Discord’s quest page. Stop the helper when finished. Source 2 is RPC profile display and does not confirm quest progress. Completion is not guaranteed.\n\n' +
        'On startup the app checks GitHub for a newer release, verifies its signature and installs it in the background. The app closes briefly and reopens after installation. Network failures do not prevent normal use. User files stay in `%LOCALAPPDATA%/DiscordQuestHelper/`.\n\n' +
        '### Türkçe\n\n' +
        'Setup dosyasını çalıştırın ve Başlat menüsünden uygulamayı açın. Discord açıkken görevi kabul edin, oyunu arayın ve varsa 1. kaynağı kullanın. İlerlemeyi Discord’dan kontrol edin.\n\n' +
        'Uygulama açılışta yeni sürümü kontrol eder; imzalı güncellemeyi arka planda yükleyip yeniden açılır. İnternet hatası uygulamayı kilitlemez. Oluşturulan oyunlar ve ayarlar güncellemeden ayrı tutulur.\n';
}

async function setVersion(version) {
    const packagePath = join(root, 'package.json');
    const packageJson = JSON.parse(await readFile(packagePath, 'utf8'));
    packageJson.version = version;
    await writeFile(packagePath, `${JSON.stringify(packageJson, null, 2)}\n`);
    const configPath = join(root, 'src-tauri/tauri.conf.json');
    const config = JSON.parse(await readFile(configPath, 'utf8'));
    config.version = version;
    await writeFile(configPath, `${JSON.stringify(config, null, 2)}\n`);
    for (const [directory, packageName] of [['src-tauri', 'discord-quest-helper'], ['src-win', 'src-win']]) {
        const manifest = join(root, directory, 'Cargo.toml');
        const content = await readFile(manifest, 'utf8');
        await writeFile(manifest, content.replace(/^version = "[^"]+"/m, `version = "${version}"`));
        const lock = join(root, directory, 'Cargo.lock');
        const lockContent = await readFile(lock, 'utf8');
        const pattern = new RegExp(`(\\[\\[package\\]\\]\\r?\\nname = "${packageName}"\\r?\\nversion = ")[^"]+`);
        if (!pattern.test(lockContent)) throw new Error(`Missing ${packageName} in lockfile`);
        await writeFile(lock, lockContent.replace(pattern, (_, prefix) => `${prefix}${version}`));
    }
    const resource = join(root, 'src-win/embed_icon.rc');
    const resourceContent = await readFile(resource, 'utf8');
    const numbers = version.split('.').join(',');
    await writeFile(resource, resourceContent
        .replace(/^(FILEVERSION|PRODUCTVERSION) .+$/gm, `$1 ${numbers},0`)
        .replace(/(VALUE "(?:FileVersion|ProductVersion)", ")[^"]+/g, (_, prefix) => `${prefix}${version}.0`));
}

async function main() {
    const [command, ...args] = process.argv.slice(2);
    const option = name => {
        const index = args.indexOf(`--${name}`);
        return index === -1 ? undefined : args[index + 1];
    };
    if (command === 'prepare') {
        const releases = JSON.parse(await readFile(option('releases'), 'utf8'));
        const date = option('date') || releaseDate();
        const tags = releases.map(item => typeof item === 'string' ? item : item.tag_name || item.tagName);
        if (option('tags')) {
            const existingTags = JSON.parse(await readFile(option('tags'), 'utf8'));
            tags.push(...existingTags.map(item => typeof item === 'string' ? item : item.name));
        }
        const version = nextVersion(date, tags);
        await setVersion(version);
        const info = { version, date, tag: `v${version}`, setup: setupName(version) };
        await writeFile(option('info'), `${JSON.stringify(info, null, 2)}\n`);
        if (process.env.GITHUB_OUTPUT) {
            for (const [key, value] of Object.entries(info)) await appendFile(process.env.GITHUB_OUTPUT, `${key}=${value}\n`);
        }
        console.log(`Release ${version} (${date})`);
    } else if (command === 'package') {
        const info = JSON.parse(await readFile(option('info'), 'utf8'));
        const directory = resolve(option('artifacts'));
        const signature = await readFile(join(directory, `${info.setup}.sig`), 'utf8');
        await writeFile(join(directory, 'latest.json'), `${JSON.stringify(updateManifest(info.version, signature), null, 2)}\n`);
        await writeFile(join(directory, 'release-notes.md'), notes(info.version, info.date));
        const files = (await readdir(directory)).filter(name => name !== 'SHA256SUMS.txt' && name !== 'release-notes.md').sort();
        const sums = [];
        for (const name of files) {
            const digest = createHash('sha256').update(await readFile(join(directory, name))).digest('hex');
            sums.push(`${digest}  ${name}`);
        }
        await writeFile(join(directory, 'SHA256SUMS.txt'), `${sums.join('\n')}\n`);
        console.log(`Prepared ${info.setup}, signature, latest.json and checksums`);
    } else {
        throw new Error('Usage: release.mjs prepare --releases <json> --info <json> | package --artifacts <dir> --info <json>');
    }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
