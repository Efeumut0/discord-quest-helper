import { readFile } from 'node:fs/promises';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export function assertCompatible(name, rustVersion, jsVersion) {
    const minor = value => value.match(/^(\d+)\.(\d+)\./)?.slice(1).join('.');
    if (!minor(rustVersion) || minor(rustVersion) !== minor(jsVersion)) {
        throw new Error(`${name}: Rust ${rustVersion} and JavaScript ${jsVersion} must use the same major/minor version`);
    }
}

async function check() {
    const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
    const lock = await readFile(join(root, 'src-tauri/Cargo.lock'), 'utf8');
    for (const [crate, npm] of [
        ['tauri', '@tauri-apps/api'],
        ['tauri-plugin-dialog', '@tauri-apps/plugin-dialog'],
        ['tauri-plugin-opener', '@tauri-apps/plugin-opener'],
    ]) {
        const rustVersion = lock.match(new RegExp(`name = "${crate}"\\r?\\nversion = "([^"]+)"`))?.[1];
        if (!rustVersion) throw new Error(`Missing ${crate} in Cargo.lock`);
        const jsPackage = JSON.parse(await readFile(join(root, 'node_modules', npm, 'package.json'), 'utf8'));
        assertCompatible(crate, rustVersion, jsPackage.version);
        console.log(`${crate}: Rust ${rustVersion}, JavaScript ${jsPackage.version}`);
    }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    check().catch(error => { console.error(error.message); process.exitCode = 1; });
}
