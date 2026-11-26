import { Game } from '@/types/types';
import { tryOnMounted, useAsyncState } from '@vueuse/core';
import { ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useGlobalState } from './state';
import { useNewGames } from './new-games';

export function useFetchGameList() {
    const { addLog } = useGlobalState();
    const { syncNewGames } = useNewGames();

    function parseGameListResponse(response: unknown) {
        if (typeof response === 'string') {
            return JSON.parse(response) as Game[];
        }

        return response as Game[] | unknown[] | undefined;
    }

    function isValidGameList(data: unknown): data is Game[] {
        return Array.isArray(data)
            && data.length > 0
            && typeof data[0] === 'object'
            && data[0] !== null
            && 'aliases' in data[0]
            && 'name' in data[0]
            && 'executables' in data[0];
    }

    async function fetchGameListFromDiscord (){
        addLog('Fetching game list directly from Discord...');
        const response = await invoke('fetch_gamelist_from_discord');
        return parseGameListResponse(response);
    };


    const {
        state: gameListFromDiscord,
        error: errorDiscord,
        isReady: isReadyDiscord,
        execute: executeDiscord,
        isLoading: isLoadingDiscord
    } = useAsyncState(fetchGameListFromDiscord, [], {
        immediate: false,
        resetOnExecute: true,
    });
    const {
        state: bundledGameList,
        error: errorBundled,
        isReady: isReadyBundled,
        execute: executeBundled,
        isLoading: isLoadingBundled
    } = useAsyncState(() => {
        const result = import('../assets/gamelist.json').then(res=>res.default);
        addLog('Fetching bundled game list for fallback...');
        return result;
    }, [], {
        immediate: false,
        resetOnExecute: true,
    });

    const fetchError = ref<string | null>(null);
    const gameDB = ref<Game[]>([]);
    const allFetchDone = ref(false);

    watch(() => isReadyDiscord.value, async (newVal) => {
        addLog('debug','isReadyDiscord: ' + newVal);
    })

    watch(() => isReadyBundled.value, async (newVal) => {
        addLog('debug','isReadyBundled: ' + newVal);
    });

    let timeoutId: ReturnType<typeof setTimeout> | null = null;
    async function fetchGameList() {
        if (timeoutId) {
            clearTimeout(timeoutId);
            timeoutId = null;
        }
        fetchError.value = null;
        allFetchDone.value = false;
        addLog('Fetching game list...');

        const errors: string[] = [];
        let selectedGameList: Game[] | null = null;
        let selectedSource = '';

        await Promise.allSettled([executeDiscord(), executeBundled()]);

        if (isValidGameList(gameListFromDiscord.value)) {
            selectedGameList = gameListFromDiscord.value as Game[];
            selectedSource = 'Discord';
        } else if (errorDiscord.value) {
            errors.push(`Discord: ${String(errorDiscord.value)}`);
        } else {
            errors.push('Discord: invalid or empty game list');
        }

        if (!selectedGameList) {
            if (isValidGameList(bundledGameList.value)) {
                selectedGameList = bundledGameList.value as Game[];
                selectedSource = 'bundled fallback';
                fetchError.value = errors.join(' | ');
                addLog('warning', `Using bundled game list because online sources failed. ${fetchError.value}`);
                addLog('warning', `Canlı Discord listesine ulaşılamadı; yerel katalog kullanılıyor: ${fetchError.value}`);
            } else {
                if (errorBundled.value) {
                    errors.push(`bundled fallback: ${String(errorBundled.value)}`);
                } else {
                    errors.push('bundled fallback: invalid or empty game list');
                }
            }
        }

        if (!selectedGameList) {
            fetchError.value = errors.join(' | ');
            addLog('error', `Oyun listesi yüklenemedi: ${fetchError.value}`);
            selectedGameList = [];
        }

        gameDB.value = selectedGameList;
        addLog(`Using game list from ${selectedSource}. ${selectedGameList.length} entries.`);

        if (gameDB.value.length > 0) {
            await syncNewGames(gameDB.value);
        }

        // Keep the loading indicator visible while the game list renders.

        timeoutId = setTimeout(() => {
            allFetchDone.value = true;
        }, 1800);

    }

    watch(allFetchDone, (newVal) => {
        if (newVal && timeoutId) {
            clearTimeout(timeoutId);
        }
    });

    tryOnMounted(async () => {
        await fetchGameList();
    });


    return {
        gameListFromDiscord,
        bundledGameList,
        fetchError,
        isReadyDiscord,
        isReadyBundled,
        gameDB,
        fetchGameList,
        isLoadingDiscord,
        isLoadingBundled,
        allFetchDone
    }
}
