import { EXECUTABLE_OS } from '@/constants/constants';
import type { Game, GameExecutable } from '@/types/types';

const ILLEGAL_PATH_CHARS = ['>', '<', ':', '"', '|', '?', '*'];
const PATH_SEP = '\\';

export function splitExecutableName(executable: GameExecutable) {
    const allSections = (executable.name || '').split(/\\|\//);
    const last = allSections[allSections.length - 1];
    const name = last?.split('.').slice(0, -1).join('.') || last;
    return [
        ...allSections.slice(0, -1),
        name,
    ];
}

export function getExecutablePath(executable: GameExecutable) {
    const allSections = (executable.name || '').split(/\\|\//);
    return allSections.slice(0, -1).join(PATH_SEP);
}

export function getExecutableFilename(executable: GameExecutable) {
    return (executable.name || '').split(/\\|\//).pop();
}

export function hasIllegalPathChars(executablePath: string) {
    return ILLEGAL_PATH_CHARS.some(char => (executablePath || '').includes(char));
}

export function isSupportedExecutable(executable: GameExecutable) {
    // Cached catalogs may still contain the retired marker. Reject them at the boundary.
    const legacyFallback = 'is_app_id_fallback' in executable && executable.is_app_id_fallback;
    return !legacyFallback
        && executable.os !== EXECUTABLE_OS.LINUX
        && executable.os !== EXECUTABLE_OS.DARWIN
        && !hasIllegalPathChars(executable.name)
        && Boolean(getExecutableFilename(executable));
}

export function getSupportedExecutables(game: Game) {
    return (game.executables || []).filter(isSupportedExecutable);
}

export interface SourcedExecutable extends GameExecutable {
    sourceNumber: 1 | 2;
}

const SOURCE_PREFIX = /^(?:[1-4]\.Source|2\.OpenAsar|3\.Steam)(?:[/\\]|$)/i;

export function getSourceExecutables(game: Game, sourceNumber: 1 | 2): SourcedExecutable[] {
    const seen = new Set<string>();
    const registered = getSupportedExecutables(game).filter(executable =>
        !SOURCE_PREFIX.test(executable.path ?? '') || /^1\.Source(?:[/\\]|$)/i.test(executable.path ?? ''),
    ).map(executable => ({
        ...executable,
        path: (executable.path || getExecutablePath(executable)).replace(/^1\.Source(?:[/\\]|$)/i, ''),
    })).filter(executable => {
        const key = `${executable.path}/${getExecutableFilename(executable)}`.replace(/\\/g, '/').toLowerCase();
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
    });
    const cleanName = (game.name || '').replace(/[^a-zA-Z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'game';
    const candidates: GameExecutable[] = sourceNumber === 1 ? registered :
        [`${cleanName}.exe`, `${cleanName}-Win64-Shipping.exe`].map(name => ({
            name, filename: name, os: EXECUTABLE_OS.WINDOWS, is_launcher: false,
        }));

    return candidates.map(executable => {
        const payload = getExecutableLaunchPayload(executable);
        const path = `${sourceNumber}.Source${payload.path ? `/${payload.path}` : ''}`.replace(/\\/g, '/');
        const existing = (game.executables || []).find(item =>
            (item.path ?? '').replace(/\\/g, '/').toLowerCase() === path.toLowerCase()
            && (item.filename ?? getExecutableFilename(item))?.toLowerCase() === payload.filename?.toLowerCase(),
        );
        return {
            ...payload, path, sourceNumber,
            is_installed: existing?.is_installed ?? false,
            is_running: existing?.is_running ?? false,
        };
    });
}

export function getPlayableExecutables(game: Game) {
    return [...getSourceExecutables(game, 1), ...getSourceExecutables(game, 2)];
}

/** Include old unsourced process locations for discovery/stop, without creating new files there. */
export function getProcessCandidateExecutables(game: Game): GameExecutable[] {
    const baseExecutables = getSupportedExecutables(game).filter(executable =>
        !SOURCE_PREFIX.test(executable.path ?? ''),
    ).map(getExecutableLaunchPayload);
    return [...getPlayableExecutables(game), ...baseExecutables];
}

export function hasRegisteredExecutable(game: Game) {
    return getSupportedExecutables(game).length > 0;
}

export function getExecutableLaunchPayload(executable: GameExecutable): GameExecutable {
    const rawPath = executable.path !== undefined && executable.path !== ''
        ? executable.path
        : getExecutablePath(executable);
    return {
        ...executable,
        path: rawPath,
        segments: splitExecutableName(executable).length,
        filename: getExecutableFilename(executable),
    };
}
