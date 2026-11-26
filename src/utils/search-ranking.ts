import type { Game } from '@/types/types';

export type GameSortMode = 'az' | 'popular' | 'id';

export interface SearchPriorityItemLike {
    name: string;
    normalized_name: string;
    rank: number;
    score: number;
    source: string;
}

export interface SearchPriorityLookup {
    byName: Map<string, SearchPriorityItemLike>;
    byToken: Map<string, SearchPriorityItemLike[]>;
}

export function createSearchPriorityLookup(items: SearchPriorityItemLike[]): SearchPriorityLookup {
    const byName = new Map<string, SearchPriorityItemLike>();
    const byToken = new Map<string, SearchPriorityItemLike[]>();

    for (const item of items) {
        const normalizedName = normalizeSearchValue(item.normalized_name || item.name);
        if (!normalizedName) continue;
        byName.set(normalizedName, item);

        const tokens = normalizedName.split(' ').filter(token => token.length > 2);
        const keys = new Set<string>(tokens);
        if (tokens.length > 1) {
            keys.add(tokens.slice(0, 2).join(' '));
        }

        for (const key of keys) {
            const bucket = byToken.get(key) ?? [];
            if (bucket.length < 80) {
                bucket.push(item);
                byToken.set(key, bucket);
            }
        }
    }

    return {
        byName,
        byToken,
    };
}

export function normalizeSearchValue(value: string | null | undefined) {
    return (value ?? '')
        .normalize('NFKD')
        .replace(/[\u0300-\u036f]/g, '')
        .replace(/[©®™]/g, '')
        .replace(/&/g, ' and ')
        .toLocaleLowerCase('tr-TR')
        .replace(/[^a-z0-9ğüşöçıİ\s]+/gi, ' ')
        .replace(/\s+/g, ' ')
        .trim();
}

function getCandidateSearchScore(candidate: string, query: string, isPrimary: boolean) {
    const value = normalizeSearchValue(candidate);
    if (!value || !query) return Number.NEGATIVE_INFINITY;

    const primaryBonus = isPrimary ? 6000 : 0;
    const lengthBonus = Math.max(0, 3000 - value.length * 15);
    const noisePenalty = getNoisePenalty(value);

    if (value === query) return 120000 + primaryBonus + lengthBonus - noisePenalty;
    if (value.startsWith(query)) return 95000 + primaryBonus + lengthBonus - noisePenalty;
    if (value.split(' ').some(word => word.startsWith(query))) {
        return 76000 + primaryBonus + lengthBonus - noisePenalty;
    }
    if (value.includes(query)) return 60000 + primaryBonus + lengthBonus - noisePenalty;

    const queryTokens = query.split(' ').filter(Boolean);
    if (queryTokens.length > 1 && queryTokens.every(token => value.includes(token))) {
        return 48000 + primaryBonus + lengthBonus - noisePenalty;
    }

    return Number.NEGATIVE_INFINITY;
}

function getNoisePenalty(value: string) {
    const noisyWords = ['demo', 'beta', 'teaser', 'prologue', 'episode', 'soundtrack', 'dlc', 'trailer'];
    return noisyWords.some(word => value.includes(word)) ? 18000 : 0;
}

function getBestSearchScore(candidates: string[], query: string) {
    if (!query) return 0;

    return candidates.reduce((bestScore, candidate, index) => {
        return Math.max(bestScore, getCandidateSearchScore(candidate, query, index === 0));
    }, Number.NEGATIVE_INFINITY);
}

export function getPopularityScore(candidates: string[], lookup: SearchPriorityLookup) {
    let bestScore = 0;
    const normalizedCandidates = candidates
        .map(candidate => normalizeSearchValue(candidate))
        .filter(Boolean);

    for (const candidate of normalizedCandidates) {
        const checkedPriorityNames = new Set<string>();
        const exact = lookup.byName.get(candidate);
        if (exact) {
            bestScore = Math.max(bestScore, exact.score);
            continue;
        }

        const tokens = candidate.split(' ').filter(token => token.length > 2);
        const keys = new Set<string>(tokens);
        if (tokens.length > 1) {
            keys.add(tokens.slice(0, 2).join(' '));
        }

        for (const key of keys) {
            const priorities = lookup.byToken.get(key) ?? [];
            for (const priority of priorities) {
                const priorityName = normalizeSearchValue(priority.normalized_name || priority.name);
                if (!priorityName || checkedPriorityNames.has(priorityName)) continue;
                checkedPriorityNames.add(priorityName);

                if (candidate === priorityName || candidate.includes(priorityName) || priorityName.includes(candidate)) {
                    bestScore = Math.max(bestScore, priority.score * 0.82);
                }
            }
        }

        const firstTokenBucket = tokens[0] ? lookup.byToken.get(tokens[0]) ?? [] : [];
        for (const priority of firstTokenBucket.slice(0, 12)) {
            const priorityName = normalizeSearchValue(priority.normalized_name || priority.name);
            if (!priorityName || checkedPriorityNames.has(priorityName)) continue;
            checkedPriorityNames.add(priorityName);
            if (candidate === priorityName || candidate.includes(priorityName) || priorityName.includes(candidate)) {
                bestScore = Math.max(bestScore, priority.score * 0.82);
            }
        }
    }

    return bestScore;
}

function compareText(a: string, b: string) {
    return a.localeCompare(b, 'tr', { numeric: true, sensitivity: 'base' });
}

function compareBySortMode<T>(
    a: { item: T; name: string; id: string; popularity: number },
    b: { item: T; name: string; id: string; popularity: number },
    sortMode: GameSortMode,
) {
    if (sortMode === 'popular') {
        return (b.popularity - a.popularity) || compareText(a.name, b.name) || compareText(a.id, b.id);
    }

    if (sortMode === 'id') {
        return compareText(a.id, b.id) || compareText(a.name, b.name);
    }

    return compareText(a.name, b.name) || compareText(a.id, b.id);
}

export function rankGames(
    games: Game[],
    queryText: string,
    sortMode: GameSortMode,
    lookup: SearchPriorityLookup,
) {
    const query = normalizeSearchValue(queryText);
    const ranked = games
        .map(game => {
            const candidates = [
                game.name,
                game.id,
                ...(game.aliases ?? []),
                ...game.executables.map(executable => executable.name),
            ];

            return {
                item: game,
                id: game.id,
                name: game.name,
                searchScore: getBestSearchScore(candidates, query),
                popularity: getPopularityScore([game.name, ...(game.aliases ?? [])], lookup),
            };
        })
        .filter(entry => !query || Number.isFinite(entry.searchScore));

    ranked.sort((a, b) => {
        if (query) {
            return (b.searchScore - a.searchScore)
                || (b.popularity - a.popularity)
                || compareBySortMode(a, b, sortMode);
        }

        return compareBySortMode(a, b, sortMode);
    });

    return ranked.map(entry => entry.item);
}

export function rankGeneratedGames<T extends { app_id: string; name: string | null }>(
    games: T[],
    queryText: string,
    sortMode: GameSortMode,
    lookup: SearchPriorityLookup,
    getExtraCandidates: (game: T) => string[] = () => [],
) {
    const query = normalizeSearchValue(queryText);
    const ranked = games
        .map(game => {
            const name = game.name || game.app_id;
            const candidates = [name, game.app_id, ...getExtraCandidates(game)];

            return {
                item: game,
                id: game.app_id,
                name,
                searchScore: getBestSearchScore(candidates, query),
                popularity: getPopularityScore([name], lookup),
            };
        })
        .filter(entry => !query || Number.isFinite(entry.searchScore));

    ranked.sort((a, b) => {
        if (query) {
            return (b.searchScore - a.searchScore)
                || (b.popularity - a.popularity)
                || compareBySortMode(a, b, sortMode);
        }

        return compareBySortMode(a, b, sortMode);
    });

    return ranked.map(entry => entry.item);
}
