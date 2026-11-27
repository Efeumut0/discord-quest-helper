<script setup lang="ts">
import { ref, computed, useTemplateRef, shallowRef, provide, onMounted, onUnmounted, onActivated, onDeactivated, watch } from 'vue';
import { onClickOutside, refDebounced } from '@vueuse/core';
import { useFuse } from '@vueuse/integrations/useFuse'
import { invoke } from '@tauri-apps/api/core';
import { randomString } from '@/utils/random-string';
import { GameActionsProvider, GameExecutable, type Game } from '@/types/types';
import IconVerified from '@/components/IconVerified.vue';
import GameExecutables from '@/components/GameExecutables.vue';
import { GameActionsKey, EXECUTABLE_OS } from '@/constants/constants';
import { useFetchGameList } from '@/composables/fetch-gamelist';
import { UseFuseOptions } from '@vueuse/integrations';
import Fuse from 'fuse.js';
import { useGlobalState } from '@/composables/state';
import TimedNotification from '@/components/TimedNotification.vue';
import { LoaderCircle, Play, Square } from '@lucide/vue';
import { getExecutableLaunchPayload, getProcessCandidateExecutables, getExecutablePath, getExecutableFilename } from '@/utils/game-executables';
import { useSearchPriority } from '@/composables/search-priority';
import { friendlyError } from '@/composables/friendly-errors';
import { rankGames } from '@/utils/search-ranking';

type DialogKey =
    'none' |
    'rpc_message_1'|
    'no_game_selected';

interface GameProcessQuery {
    app_id: string;
    path: string;
    executable_name: string;
}

interface GameProcessStatus extends GameProcessQuery {
    full_path: string;
    installed: boolean;
    running: boolean;
    process_ids: number[];
}

interface RunningGeneratedGameProcess extends GameProcessQuery {
    full_path: string;
    process_ids: number[];
}

interface ActiveQuestCreatedGame {
    id: string;
    name: string;
    quest_ids: string[];
    quest_names: string[];
    executable_count: number;
    match_method: string;
    match_score: number;
    fallback_executable_used: boolean;
    has_play_quest: boolean;
    has_stream_quest: boolean;
}

interface ActiveQuestCreateResponse {
    active_quest_count: number;
    matched_game_count: number;
    created_executable_count: number;
    skipped_executable_count: number;
    created_games: ActiveQuestCreatedGame[];
    sources: string[];
    source_errors: string[];
    creation_errors: string[];
}

const {
    gameDB,
    isLoadingBundled,
    isLoadingDiscord,
    fetchGameList,
    isReadyBundled,
    isReadyDiscord,
    allFetchDone,
} = useFetchGameList()
const { addLog, t } = useGlobalState();
const {
    priorityLookup,
    isLoading: isLoadingPriority,
    isReady: isReadyPriority,
    refreshed: isPriorityRefreshed,
    usedCache: isPriorityUsingCache,
} = useSearchPriority();
const shouldShowNotificationContainer = computed(() => {
    return isLoadingDiscord.value || isLoadingBundled.value ||
           isLoadingPriority.value ||
           (isReadyDiscord.value || isReadyBundled.value || isReadyPriority.value);
});

const dialogRef = useTemplateRef<HTMLDialogElement>('dialogRef');
const searchResultContainerRef = useTemplateRef<HTMLElement>('searchResultContainerRef')
const dialogMessage = ref('');
const isDialogOpen = ref(false);
const dialogKey = ref<DialogKey>('none')
const isConnectedToRPC = ref(false);
const isConnecting = ref(false);
const isSyncingProcesses = ref(false);
const isDiscoveringRunningProcesses = ref(false);
const isCreatingActiveQuestGames = ref(false);
let processSyncTimer: number | undefined;
let runningDiscoveryTimer: number | undefined;

const searchQuery = shallowRef('');
const debouncedSearchQuery = refDebounced(searchQuery, 300)

const searchResultsIsOpen = ref(false);
const isOnSearchResults = ref(false);

const currentlyPlaying = ref<string | null>(null);

onClickOutside(searchResultContainerRef, () => {
    searchResultsIsOpen.value = false;
})

const COPYRIGHT_SYMBOL = '\u00A9';
const TRADEMARK_SYMBOL = '\u2122';
const REGISTERED_SYMBOL = '\u00AE';
const ignoredSymbols = [COPYRIGHT_SYMBOL, TRADEMARK_SYMBOL, REGISTERED_SYMBOL];
const ignoredSymbolsRegex = new RegExp(`[${ignoredSymbols.join('')}]`, 'g');
const fuseOptions = computed<UseFuseOptions<Game>>(() => ({
    fuseOptions: {
        // Prioritize name and active quests, then aliases, id, and executables
        keys: [
            { name: 'name', weight: 0.6 },
            { name: 'active_quest_names', weight: 0.5 },
            { name: 'id', weight: 0.4 },
            { name: 'active_quest_ids', weight: 0.4 },
            { name: 'aliases', weight: 0.2 },
            { name: 'executables.name', weight: 0.1 },
        ],
        getFn: (obj: any, path: string[] | string) => {
            const value = Fuse.config.getFn(obj, path);
            return typeof value === "string"
            ? value.replace(ignoredSymbolsRegex, "")
            : value;
        },
        isCaseSensitive: false,
        threshold: 0.5,
        // A score of 0 indicates a perfect match, while a score of 1 indicates a complete mismatch
        includeScore: true,
        includeMatches: false
    },
    resultLimit: 500,
    matchAllWhenSearchEmpty: false,
}));

const { results: rawSearchResults } = useFuse(debouncedSearchQuery, gameDB, fuseOptions)
const searchResults = computed(() => {
    const query = debouncedSearchQuery.value.trim().toLowerCase();
    if (!query) return [];

    // Direct matches by ID, active quest ID, or active quest name
    const directMatches = gameDB.value.filter(game => {
        if (game.id.toLowerCase() === query || game.id.toLowerCase().includes(query)) return true;
        if (game.active_quest_ids?.some(qid => qid.toLowerCase() === query || qid.toLowerCase().includes(query))) return true;
        if (game.active_quest_names?.some(qname => qname.toLowerCase().includes(query))) return true;
        return false;
    });

    const rankedGames = rankGames(
        rawSearchResults.value.map(result => result.item),
        debouncedSearchQuery.value,
        'popular',
        priorityLookup.value,
    );

    const combined: Game[] = [...directMatches];
    for (const g of rankedGames) {
        if (!combined.some(existing => existing.id === g.id)) {
            combined.push(g);
        }
    }

    return combined.slice(0, 15).map(item => ({ item }));
});

const gameList = ref<Game[]>([]);
const isStartingAllGames = ref(false);
const isStoppingAllGames = ref(false);
const selectedGameId = ref<string | null | undefined>(null);

const selectedGame = computed(() => {
    if (!selectedGameId.value) return null;
    const found = gameList.value.find(g => g.uid === selectedGameId.value);
    return found || null;
});

// A quest can ask for play, for streaming, or for both. Only a stream-only game
// is moved aside: it needs the user to share the window in a voice channel, so
// starting it on its own achieves nothing.
function isStreamOnlyGame(game: Game) {
    return Boolean(game.has_stream_quest) && !game.has_play_quest;
}

const orderedGameList = computed(() => [
    ...gameList.value.filter(game => !isStreamOnlyGame(game)),
    ...gameList.value.filter(game => isStreamOnlyGame(game)),
]);

const streamSectionStart = computed(() =>
    orderedGameList.value.findIndex(game => isStreamOnlyGame(game)),
);

const hasRunningGames = computed(() => gameList.value.some(game =>
    game.is_running || getAllCandidateExecutables(game).some(executable => executable.is_running || isExecutableRunning(executable)),
));

const hasStartableGames = computed(() => gameList.value.some(game => {
    if (isStreamOnlyGame(game)) return false;
    const candidates = getAllCandidateExecutables(game);
    return candidates.length > 0 && !candidates.some(executable => executable.is_running || isExecutableRunning(executable));
}));

function errorMessage(error: unknown) {
    const raw = error instanceof Error ? error.message : String(error);
    return friendlyError(raw, t);
}

function formatMessage(template: string, values: Record<string, string | number>) {
    return Object.entries(values).reduce(
        (message, [key, value]) => message.replace(`{${key}}`, String(value)),
        template,
    );
}

function normalizeProcessKeyPart(value: string | undefined) {
    return (value ?? '').replace(/\//g, '\\').toLowerCase();
}

function processStatusKey(appId: string, executablePath: string | undefined, executableName: string | undefined) {
    return [
        normalizeProcessKeyPart(appId),
        normalizeProcessKeyPart(executablePath),
        normalizeProcessKeyPart(executableName),
    ].join('\u0001');
}

function createListGame(game: Game): Game {
    const executables = (game.executables || []).map(executable => ({ ...executable }));

    return {
        ...game,
        uid: randomString(),
        is_installed: game.is_installed ?? false,
        is_running: game.is_running ?? false,
        executables,
    };
}

function getAllCandidateExecutables(game: Game): GameExecutable[] {
    return getProcessCandidateExecutables(game);
}

function buildProcessQueries() {
    const queries: GameProcessQuery[] = [];

    for (const game of gameList.value) {
        for (const executable of getAllCandidateExecutables(game)) {
            const launchExecutable = getExecutableLaunchPayload(executable);
            if (!launchExecutable.filename) {
                continue;
            }

            queries.push({
                app_id: game.id,
                path: launchExecutable.path ?? '',
                executable_name: launchExecutable.filename,
            });
        }
    }

    return queries;
}

function applyProcessStatuses(statuses: GameProcessStatus[]) {
    const statusByKey = new Map(statuses.map(status => [
        processStatusKey(status.app_id, status.path, status.executable_name),
        status,
    ]));
    let firstRunningGameId: string | null = null;

    for (const game of gameList.value) {
        let gameIsInstalled = false;
        let gameIsRunning = false;

        const candidates = getAllCandidateExecutables(game);
        for (const executable of candidates) {
            const launchExecutable = getExecutableLaunchPayload(executable);
            const status = statusByKey.get(processStatusKey(
                game.id,
                launchExecutable.path,
                launchExecutable.filename,
            ));

            if (!status) {
                continue;
            }

            const targetPath = (launchExecutable.path ?? '').toLowerCase();
            const targetFilename = (launchExecutable.filename ?? launchExecutable.name)?.toLowerCase() ?? '';
            let existingInGame = (game.executables || []).find(e =>
                (e.path ?? '').toLowerCase() === targetPath &&
                (e.filename ?? e.name)?.toLowerCase() === targetFilename
            );

            if (status.installed || status.running) {
                if (!existingInGame) {
                    existingInGame = {
                        ...executable,
                        path: launchExecutable.path,
                        filename: launchExecutable.filename,
                        is_installed: status.installed,
                        is_running: status.running,
                    };
                    game.executables.push(existingInGame);
                } else {
                    existingInGame.is_installed = status.installed;
                    existingInGame.is_running = status.running;
                }
            } else if (existingInGame) {
                existingInGame.is_installed = false;
                existingInGame.is_running = false;
            }

            executable.is_installed = status.installed;
            executable.is_running = status.running;
            gameIsInstalled = gameIsInstalled || status.installed;
            gameIsRunning = gameIsRunning || status.running;
        }

        game.is_installed = gameIsInstalled;
        game.is_running = gameIsRunning;

        if (!firstRunningGameId && gameIsRunning) {
            firstRunningGameId = game.id;
        }
    }

    currentlyPlaying.value = firstRunningGameId;
}

async function refreshGameProcessStates() {
    if (isSyncingProcesses.value) {
        return;
    }

    const requests = buildProcessQueries();
    if (requests.length === 0) {
        currentlyPlaying.value = null;
        return;
    }

    isSyncingProcesses.value = true;
    try {
        const statuses = await invoke<GameProcessStatus[]>('get_game_process_statuses', {
            requests,
        });
        applyProcessStatuses(statuses);
    } catch (error) {
        console.warn('Failed to sync game processes:', error);
    } finally {
        isSyncingProcesses.value = false;
    }
}

async function discoverRunningGeneratedGames() {
    if (isDiscoveringRunningProcesses.value || gameDB.value.length === 0) {
        return;
    }

    isDiscoveringRunningProcesses.value = true;
    try {
        const runningProcesses = await invoke<RunningGeneratedGameProcess[]>('get_running_generated_game_processes');
        let changed = false;

        for (const process of runningProcesses) {
            let game = gameList.value.find(g => normalizeProcessKeyPart(g.id) === normalizeProcessKeyPart(process.app_id));

            if (!game) {
                const dbGame = gameDB.value.find(g => normalizeProcessKeyPart(g.id) === normalizeProcessKeyPart(process.app_id));
                if (!dbGame) {
                    continue;
                }
                game = createListGame(dbGame);
                gameList.value.push(game);
                changed = true;
            }

            const targetPath = (process.path ?? '').toLowerCase();
            const targetFilename = (process.executable_name ?? '')?.toLowerCase() ?? '';
            let existingExe = (game.executables || []).find(e =>
                (e.path ?? '').toLowerCase() === targetPath &&
                (e.filename ?? e.name)?.toLowerCase() === targetFilename
            );

            if (!existingExe) {
                existingExe = {
                    name: process.executable_name,
                    filename: process.executable_name,
                    path: process.path,
                    os: EXECUTABLE_OS.WINDOWS,
                    is_launcher: false,
                    is_installed: true,
                    is_running: true,
                };
                game.executables.push(existingExe);
                changed = true;
            } else {
                existingExe.is_installed = true;
                existingExe.is_running = true;
            }
            game.is_running = true;
            game.is_installed = true;
        }

        if (changed || runningProcesses.length > 0) {
            await refreshGameProcessStates();
        }
    } catch (error) {
        console.warn('Failed to discover running generated games:', error);
    } finally {
        isDiscoveringRunningProcesses.value = false;
    }
}

function closeSearchResults() {
    searchResultsIsOpen.value = false;
}
function openSearchResults() {
    searchResultsIsOpen.value = true;
}

function addGameToList(game: Game) {
    if (!gameList.value.some(g => g.id === game.id)) {
        gameList.value.push(createListGame(game));
    }

    void refreshGameProcessStates();
    closeSearchResults();
}

function attachActiveQuestMetadata(
    gameId: string,
    questIds: string[],
    questNames: string[],
    kinds: { play: boolean; stream: boolean } = { play: true, stream: false },
) {
    const targets = [
        gameList.value.find(item => item.id === gameId),
        gameDB.value.find(item => item.id === gameId),
    ].filter((g): g is Game => Boolean(g));

    for (const game of targets) {
        const existingIds = new Set(game.active_quest_ids ?? []);
        for (const questId of questIds) {
            if (questId.trim()) existingIds.add(questId);
        }
        const existingNames = new Set(game.active_quest_names ?? []);
        for (const questName of questNames) {
            if (questName.trim()) existingNames.add(questName);
        }
        game.active_quest_ids = Array.from(existingIds);
        game.active_quest_names = Array.from(existingNames);
        game.has_play_quest = Boolean(game.has_play_quest) || kinds.play;
        game.has_stream_quest = Boolean(game.has_stream_quest) || kinds.stream;
    }
}

const canCreateActiveQuestGames = computed(() => {
    return gameDB.value.length > 0
        && !isCreatingActiveQuestGames.value;
});

function buildActiveQuestCatalog() {
    return gameDB.value.map(game => ({
        id: game.id,
        name: game.name,
        icon_hash: game.icon_hash ?? null,
        aliases: game.aliases ?? [],
        executables: (game.executables || []).map(executable => ({
            name: executable.name,
            os: executable.os,
        })),
    }));
}

async function createActiveQuestGames() {
    if (!canCreateActiveQuestGames.value) {
        return;
    }

    isCreatingActiveQuestGames.value = true;
    addLog('info', t('home.activeQuestStarting'));

    try {
        const response = await invoke<ActiveQuestCreateResponse>('create_active_quest_games', {
            games: buildActiveQuestCatalog(),
        });

        const incomingQuestIds = new Set(response.created_games.flatMap(g => g.quest_ids));
        for (const item of gameList.value) {
            if (item.active_quest_ids && item.active_quest_ids.some(id => incomingQuestIds.has(id))) {
                item.active_quest_ids = item.active_quest_ids.filter(id => !incomingQuestIds.has(id));
                if (!response.created_games.some(g => g.id === item.id)) {
                    item.active_quest_names = (item.active_quest_names || []).filter(name =>
                        !response.created_games.some(g => g.quest_names.includes(name)),
                    );
                }
            }
        }

        for (const createdGame of response.created_games) {
            addLog(
                'info',
                `${createdGame.name}: ${createdGame.match_method} (${createdGame.match_score})${createdGame.fallback_executable_used ? ' fallback exe' : ''}`,
            );

            let game = gameDB.value.find(item => item.id === createdGame.id);
            if (!game) {
                game = {
                    id: createdGame.id,
                    name: createdGame.name,
                    executables: [],
                };
            }
            if (!gameList.value.some(item => item.id === game.id)) {
                gameList.value.push(createListGame(game));
            }
            attachActiveQuestMetadata(createdGame.id, createdGame.quest_ids, createdGame.quest_names, {
                play: createdGame.has_play_quest,
                stream: createdGame.has_stream_quest,
            });
        }

        if (response.matched_game_count === 0) {
            addLog('warning', t('home.activeQuestNoMatches'));
        } else {
            addLog('info', formatMessage(t('home.activeQuestFinished'), {
                quests: response.active_quest_count,
                games: response.matched_game_count,
                executables: response.created_executable_count,
            }));
        }

        for (const error of response.source_errors) {
            addLog('warning', error);
        }
        for (const error of response.creation_errors) {
            addLog('warning', error);
        }

        await refreshGameProcessStates();
    } catch (error) {
        addLog('error', `${t('home.activeQuestFailed')} ${errorMessage(error)}`);
    } finally {
        isCreatingActiveQuestGames.value = false;
    }
}

const forceRerenderKey = ref(0);
function removeGameFromList(game: Game) {
    const gameId = game.uid;
    gameList.value = gameList.value.filter(game => game.uid !== gameId);
    if (selectedGame.value?.uid === gameId) {
        selectedGameId.value = null;
        forceRerenderKey.value++;
    }
}

function selectGame(game: Game) {
    selectedGameId.value = game?.uid;
    searchResultsIsOpen.value = false;
}

function canCreateDummyGame(game: Game | null) {
    if (!game) {
        return false;
    }
    return !game.is_installed
}

function canPlayGame(game: Game | null) {
    if (!game) {
        return false;
    }
    return (game.is_installed && !game.is_running) ?? false;
}

function isExecutableRunning(executable: GameExecutable) {
    return executable.is_running ?? false;
}
function isGameExecutableInstalled(executable: GameExecutable) {
    return executable.is_installed ?? false;
}

function isGameInstalled(game: Game | null) {
    if (!game) {
        return false;
    }
    return game.is_installed ?? false;
}

function findExecutableItem(executables: GameExecutable[], target: GameExecutable): GameExecutable | undefined {
    const targetPath = (target.path ?? getExecutablePath(target) ?? '').toLowerCase();
    const targetFilename = (target.filename ?? getExecutableFilename(target) ?? target.name)?.toLowerCase() ?? '';

    return (executables || []).find(exe => {
        const exePath = (exe.path ?? getExecutablePath(exe) ?? '').toLowerCase();
        const exeFilename = (exe.filename ?? getExecutableFilename(exe) ?? exe.name)?.toLowerCase() ?? '';
        return exePath === targetPath && exeFilename === targetFilename;
    });
}

async function createDummyGame(game: Game | null, executable: GameExecutable) {
    if (!game) {
        return;
    }
    const gameUid = game.uid;
    const gameToInstall = gameList.value.find(g => g.uid === gameUid || g.id === game.id);
    if (gameToInstall) {
        let executableItem = findExecutableItem(gameToInstall.executables, executable);
        if (!executableItem) {
            executableItem = {
                ...executable,
                path: executable.path ?? getExecutablePath(executable),
                filename: executable.filename ?? getExecutableFilename(executable),
                is_installed: false,
                is_running: false,
            };
            gameToInstall.executables.push(executableItem);
        }

        const payload =  {
            name: gameToInstall.name,
            path: executable.path ?? getExecutablePath(executable),
            executable_name: executable.filename ?? getExecutableFilename(executable),
            app_id: gameToInstall.id,
            icon_hash: gameToInstall.icon_hash ?? '',
        };
        await invoke('create_fake_game', payload);
        gameToInstall.is_installed = true;
        executableItem.is_installed = true;
        void refreshGameProcessStates();
        return true;
    }
}

async function installAndPlay({game, executable}: {game: Game, executable: GameExecutable}) {
    if (!game) {
        return;
    }
    try {
        const gameCreated = await createDummyGame(game, executable);
        if (gameCreated) {
            await playGame({game, executable});
        } else {
            addLog('error', t('home.logCreateFailed'));
        }
    } catch (error) {
        addLog('error', `${t('home.logCreateFailed')}: ${errorMessage(error)}`);
    }
}
async function playGame({game, executable}: {game: Game, executable: GameExecutable}) {
    if (!game) {
        return;
    }
    const gameUid = game.uid;
    try {
        addLog('info', `${t('home.logPlayingGame')} ${game.name}`);
        addLog('info', `${t('home.logExecutable')} ${executable.name}`);
        const gameToPlay = gameList.value.find(g => g.uid === gameUid || g.id === game.id);
        if (gameToPlay) {
            let executableItem = findExecutableItem(gameToPlay.executables, executable);
            if (!executableItem) {
                executableItem = {
                    ...executable,
                    path: executable.path ?? getExecutablePath(executable),
                    filename: executable.filename ?? getExecutableFilename(executable),
                    is_installed: true,
                    is_running: false,
                };
                gameToPlay.executables.push(executableItem);
            }

            const payload =  {
                name: game.name,
                path: executable.path ?? getExecutablePath(executable),
                executable_name: executable.filename ?? getExecutableFilename(executable),
                app_id: gameToPlay.id,
            };
            await invoke('run_background_process', payload);
            currentlyPlaying.value = game.id;
            gameToPlay.is_running = true;
            executableItem.is_running = true;
            void refreshGameProcessStates();
        }
    } catch (error) {
        addLog('error', `${t('home.logLaunchFailed')} ${errorMessage(error)}`);
    }
}

async function stopPlaying({game, executable}: {game: Game, executable: GameExecutable}) {
    if (!game) {
        return;
    }
    const gameUid = game.uid;

    currentlyPlaying.value = null;

    const gameToPlay = gameList.value.find(g => g.uid === gameUid || g.id === game.id);
    if (gameToPlay) {
        let executableItem = findExecutableItem(gameToPlay.executables, executable);
        try {
            await invoke('stop_process', {
                exec_name: executable.filename || getExecutableFilename(executable) || '',
                app_id: gameToPlay.id,
                path: executable.path ?? getExecutablePath(executable) ?? '',
            });
            addLog('info', `${t('home.logStoppedGame')} ${game.name}`);
            addLog('info', `${t('home.logStoppedExecutable')} ${executable.name}`);
        } catch (error) {
            addLog('error', `${t('home.logStopFailed')} ${errorMessage(error)}`);
        } finally {
            if (executableItem) {
                executableItem.is_running = false;
            }
            await refreshGameProcessStates();
        }
    }
}

async function startAllGames() {
    if (isStartingAllGames.value || isStoppingAllGames.value) return;
    isStartingAllGames.value = true;
    try {
        for (const game of gameList.value) {
            if (isStreamOnlyGame(game)) continue;
            const candidates = getAllCandidateExecutables(game);
            if (candidates.some(e => e.is_running || isExecutableRunning(e))) continue;
            const executable = candidates[0];
            if (!executable || executable.is_running || isExecutableRunning(executable)) continue;
            try {
                const launchExecutable = getExecutableLaunchPayload(executable);
                if (executable.is_installed) {
                    await playGame({ game, executable: launchExecutable });
                } else {
                    await installAndPlay({ game, executable: launchExecutable });
                }
                await refreshGameProcessStates();
            } catch (error) {
                addLog('error', `${game.name}: ${errorMessage(error)}`);
            }
        }
    } finally {
        await refreshGameProcessStates();
        isStartingAllGames.value = false;
    }
}

async function stopAllGames() {
    if (isStoppingAllGames.value || isStartingAllGames.value) return;
    isStoppingAllGames.value = true;
    try {
        const runningProcesses = await invoke<RunningGeneratedGameProcess[]>('get_running_generated_game_processes').catch(() => []);

        for (const game of gameList.value) {
            const runningExecutables = getAllCandidateExecutables(game).filter(executable => executable.is_running || isExecutableRunning(executable));
            for (const executable of runningExecutables) {
                try {
                    await stopPlaying({ game, executable: getExecutableLaunchPayload(executable) });
                } catch (error) {
                    addLog('error', `${game.name}: ${errorMessage(error)}`);
                }
            }
        }

        for (const proc of runningProcesses) {
            try {
                await invoke('stop_process', {
                    exec_name: proc.executable_name,
                    app_id: proc.app_id,
                    path: proc.path,
                });
            } catch {
                // Ignore if already stopped
            }
        }
    } finally {
        await refreshGameProcessStates();
        isStoppingAllGames.value = false;
    }
}

function getExecutables(game: Game) {
    return (game.executables || []).map(exe => exe.name)
}

async function handleTestRPC(game: Game | null) {
    let state = isConnectedToRPC.value ? 'disconnect' : 'connect';

    if (!game && state === 'connect') {
        showDialog('no_game_selected');
        return;
    }
    if (state === 'disconnect' || isConnecting.value) {
        isConnecting.value = true;
        try {
            await invoke('connect_to_discord_rpc_3', {
                activity_json: '{}',
                action: 'disconnect',
            });
        } catch (error) {
            addLog('error', `${t('home.logRpcFailed')} ${errorMessage(error)}`);
        } finally {
            isConnectedToRPC.value = false;
            if (game) {
                game.is_running = false;
            }
            currentlyPlaying.value = null;
            isConnecting.value = false;
        }
        return;
    }
    showDialog('rpc_message_1');
}

async function continueRPCRisk(game: Game | null) {
    if (!game) {
        return;
    }
    const gameUid = game.uid;
    const gameToTest = gameList.value.find(g => g.uid === gameUid);
    if (gameToTest) {
        isConnecting.value = true;
        hideDialog();
        try {
            await invoke('connect_to_discord_rpc_3', {
                activity_json: JSON.stringify({
                    app_id: gameToTest.id,
                }),
                action: 'connect',
            });
            isConnectedToRPC.value = true;
            gameToTest.is_running = true;
            currentlyPlaying.value = gameToTest.id;
        } catch (error) {
            addLog('error', `${t('home.logRpcFailed')} ${errorMessage(error)}`);
            isConnectedToRPC.value = false;
            gameToTest.is_running = false;
            currentlyPlaying.value = null;
        } finally {
            isConnecting.value = false;
        }
    }
}

function handleSearchBlur() {
    setTimeout(() => {
        if (!isOnSearchResults.value) {
            searchResultsIsOpen.value = false;
        }
    }, 200);
}

function showDialog(message: DialogKey) {
    isDialogOpen.value = true;
    dialogMessage.value = message;
    dialogKey.value = message;
    dialogRef.value?.showModal();
}

function hideDialog() {
    dialogRef.value?.close();
    dialogMessage.value = '';
    isDialogOpen.value = false;
}

provide<GameActionsProvider>(GameActionsKey, {
    canPlayGame,
    isGameInstalled,
    isExecutableRunning,
    isGameExecutableInstalled,
});

function startProcessSync() {
    if (processSyncTimer !== undefined || runningDiscoveryTimer !== undefined) return;
    void discoverRunningGeneratedGames();
    void refreshGameProcessStates();
    processSyncTimer = window.setInterval(() => {
        void refreshGameProcessStates();
    }, 2500);
    runningDiscoveryTimer = window.setInterval(() => {
        void discoverRunningGeneratedGames();
    }, 7500);
}

function stopProcessSync() {
    if (processSyncTimer !== undefined) {
        window.clearInterval(processSyncTimer);
        processSyncTimer = undefined;
    }
    if (runningDiscoveryTimer !== undefined) {
        window.clearInterval(runningDiscoveryTimer);
        runningDiscoveryTimer = undefined;
    }
}

onMounted(() => {
    startProcessSync();
});
onActivated(startProcessSync);
onDeactivated(stopProcessSync);
onUnmounted(() => {
    stopProcessSync();
});

watch(() => gameDB.value.length, (gameCount) => {
    if (gameCount > 0) {
        void discoverRunningGeneratedGames();
    }
});
</script>

<template>
    <div class="app-page app-page-full app-page-wide flex-col">

        <dialog id="dialog" class="dialogStyle inset-0 bg-gray-800 bg-opacity-50
        border border-gray-300 dark:border-gray-600 rounded-lg
        transition-opacity duration-300 ease-in-out z-50
        "
        style="left: 50%; top: 50%; transform: translate(-50%, -50%)"
        ref="dialogRef">
            <div class="flex flex-col items-center justify-center p-6" >
                <div class="mb-4 text-gray-500 dark:text-gray-400">
                    <div v-if="dialogKey === 'rpc_message_1'">
                        <p>
                        {{ t('home.rpcWarning1') }}
                        </p>
                        <p class="my-2">
                            {{ t('home.rpcWarning2') }}
                        </p>
                        <p>
                        {{ t('home.rpcWarning3') }}
                        </p>
                    </div>

                    <div v-if="dialogKey === 'no_game_selected'">
                        <p>
                            {{ t('home.noGameSelected') }}
                        </p>
                    </div>
                </div>
                <div class="gap-2 flex">
                    <button

                    class="
                text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300
                border border-gray-300 dark:border-gray-600 rounded-lg px-4 py-1"
                @click="hideDialog()">
                    <span  v-if="dialogKey == 'rpc_message_1'">
                        {{ t('home.cancel') }}
                    </span>
                    <span v-else>{{ t('home.ok') }}</span>
                </button>

                <button
                v-if="dialogKey === 'rpc_message_1'"
                class="text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300
                border border-gray-300 dark:border-gray-600 rounded-lg px-4 py-1"
                @click="continueRPCRisk(selectedGame)">
                    {{ t('home.acceptRisk') }}
                </button>
                </div>
            </div>
        </dialog>

        <h1 class="text-3xl font-bold text-gray-900 dark:text-white mb-6 text-center">
            {{ t('home.title') }}
        </h1>

        <Transition
            enter-active-class="transition-opacity duration-300 delay-100 ease-in-out"
            leave-active-class="transition-opacity duration-600 delay-100 ease-in-out"
            enter-from-class="opacity-0 translate-y-2 ease-in-out"
            enter-to-class="opacity-100 translate-y-0 ease-in-out"
        >
            <div class="pointer-events-none absolute top-20 left-4 z-20" v-if="shouldShowNotificationContainer && !allFetchDone">

                <Transition
                    enter-active-class="transition-opacity duration-300 delay-100 ease-in-out"
                    leave-active-class="transition-opacity duration-600 delay-100 ease-in-out"
                    enter-from-class="opacity-0 translate-y-2 ease-in-out"
                    enter-to-class="opacity-100 translate-y-0 ease-in-out"
                >
                    <div v-if="isLoadingDiscord" class="text-sm text-gray-500 dark:text-gray-400">
                        {{ t('home.fetchDiscord') }}
                        <div class="border-full h-2 w-2 bg-green-500 rounded-full inline-block ml-2 animate-pulse"></div>
                    </div>
                </Transition>
                <TimedNotification
                    :is-ready="isReadyDiscord"
                    :duration="1500"
                    container-class="text-sm text-gray-500 dark:text-gray-400"
                >
                    {{ t('home.fetchDiscordDone') }} <span class="text-green-400">✓</span>
                </TimedNotification>

                <Transition
                    enter-active-class="transition-opacity duration-300 delay-100 ease-in-out"
                    leave-active-class="transition-opacity duration-600 delay-100 ease-in-out"
                    enter-from-class="opacity-0 translate-y-2 ease-in-out"
                    enter-to-class="opacity-100 translate-y-0 ease-in-out"
                >
                    <div v-if="isLoadingBundled" class="text-sm text-gray-500 dark:text-gray-400">
                        {{ t('home.fetchBundled') }}
                        <div class="border-full h-2 w-2 bg-green-500 rounded-full inline-block ml-2 animate-pulse"></div>
                    </div>
                </Transition>
                <TimedNotification
                    :is-ready="isReadyBundled"
                    :duration="1500"
                    container-class="text-sm text-gray-500 dark:text-gray-400"
                >
                    {{ t('home.fetchBundledDone') }} <span class="text-green-400">✓</span>
                </TimedNotification>

                <Transition
                    enter-active-class="transition-opacity duration-300 delay-100 ease-in-out"
                    leave-active-class="transition-opacity duration-600 delay-100 ease-in-out"
                    enter-from-class="opacity-0 translate-y-2 ease-in-out"
                    enter-to-class="opacity-100 translate-y-0 ease-in-out"
                >
                    <div v-if="isLoadingPriority" class="text-sm text-gray-500 dark:text-gray-400">
                        {{ t('home.priorityUpdating') }}
                        <div class="border-full h-2 w-2 bg-green-500 rounded-full inline-block ml-2 animate-pulse"></div>
                    </div>
                </Transition>
                <TimedNotification
                    :is-ready="isReadyPriority && !isLoadingPriority"
                    :duration="1500"
                    container-class="text-sm text-gray-500 dark:text-gray-400"
                >
                    {{ isPriorityUsingCache && !isPriorityRefreshed ? t('home.priorityCached') : t('home.priorityUpdated') }}
                    <span class="text-green-400">✓</span>
                </TimedNotification>

            </div>
        </Transition>

        <div class="mb-8">
            <div class="relative" ref="searchResultContainerRef">
               <div class="flex flex-col gap-2 lg:block">
                 <input v-model="searchQuery" type="text" :placeholder="t('home.searchPlaceholder')"
                    class="w-full rounded-lg border border-gray-300 px-4 py-2 dark:border-gray-600 dark:bg-gray-700 dark:text-white lg:pr-[26rem] focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500"
                    @focus="openSearchResults" @blur="handleSearchBlur" />

                <div class="flex items-center justify-end gap-2 lg:absolute lg:right-2 lg:top-1/2 lg:-translate-y-1/2">
                    <button
                        type="button"
                        :disabled="!canCreateActiveQuestGames"
                        :title="t('home.createActiveQuestGames')"
                        @click="createActiveQuestGames"
                        class="px-3 py-1 text-sm bg-gray-200 dark:bg-gray-600 hover:bg-gray-300 dark:hover:bg-gray-500 text-gray-700 dark:text-white rounded-md disabled:cursor-not-allowed disabled:opacity-60">
                        <span class="wrap whitespace-nowrap text-xs">
                            {{ isCreatingActiveQuestGames ? '...' : t('home.createActiveQuestGames') }}
                        </span>
                    </button>
                    <button
                        type="button"
                        @click="fetchGameList()"
                        class="px-3 py-1 text-sm bg-gray-200 dark:bg-gray-600 hover:bg-gray-300 dark:hover:bg-gray-500 text-gray-700 dark:text-white rounded-md">
                        <span class="wrap whitespace-nowrap text-xs">
                            {{ t('home.refetch') }}
                        </span>
                    </button>
                </div>
               </div>
                <div v-if="searchResultsIsOpen" @click="isOnSearchResults = true"
                    class="absolute z-50 mt-1 w-full bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg shadow-lg max-h-60 overflow-y-auto">
                    <div v-if="searchResults.length > 0">
                        <div v-for="game in searchResults" :key="game.item.id"
                            class="p-3 hover:bg-gray-100 dark:hover:bg-gray-700 border-b border-gray-200 dark:border-gray-700 last:border-b-0">
                            <div class="flex justify-between items-center">
                                <div>
                                    <div class="font-medium text-gray-800 dark:text-white">
                                        {{ game.item.name }}
                                    </div>
                                    <div v-if="game.item.active_quest_names?.length" class="flex flex-wrap gap-1 mt-1">
                                        <span v-for="qName in game.item.active_quest_names" :key="qName" class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[11px] font-medium bg-amber-500/15 text-amber-600 dark:text-amber-400 border border-amber-500/30">
                                            🎯 {{ qName }}
                                        </span>
                                    </div>
                                    <div class="text-sm text-gray-500 dark:text-gray-400">ID: {{ game.item.id }}</div>
                                    <div class="text-xs text-gray-500 dark:text-gray-400">
                                        {{ t('home.executables') }}
                                        <ul class="list-disc list-inside">
                                            <li v-for="exe in game.item.executables" :key="exe.name"
                                                class="text-gray-500 dark:text-gray-400">
                                                <span class="font-mono">
                                                {{ exe.name }}
                                                ({{ exe.os }})</span>
                                            </li>
                                        </ul>
                                    </div>
                                </div>
                                <button @click="addGameToList(game.item)"
                                    class="ml-2 px-3 py-1 text-sm bg-indigo-600 hover:bg-indigo-700 text-white rounded-md">
                                    {{ t('home.addGame') }}
                                </button>
                            </div>
                        </div>
                    </div>

                    <div v-if="searchResults.length === 0"
                        class="p-3 hover:bg-gray-100 dark:hover:bg-gray-700 border-b border-gray-200 dark:border-gray-700 last:border-b-0 text-gray-500 dark:text-gray-400">
                        {{ t('home.searchHelp') }} <br>
                        {{ t('home.searchHelpAction') }}
                    </div>
                </div>
            </div>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-6 relative">

            <div class="home-panel bg-white dark:bg-gray-800 p-4 rounded-lg shadow">
                <div class="sticky top-0 z-10 mb-4 flex flex-wrap items-center justify-between gap-2 bg-white py-2 dark:bg-gray-800">
                    <h2 class="text-xl font-bold text-gray-900 dark:text-white">{{ t('home.games') }}</h2>
                    <div v-if="gameList.length > 0" class="flex flex-wrap items-center gap-2">
                        <button type="button" class="inline-flex items-center gap-1.5 rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-emerald-500 disabled:cursor-not-allowed disabled:opacity-50" :disabled="!hasStartableGames || isStartingAllGames || isStoppingAllGames" @click.stop="startAllGames">
                            <LoaderCircle v-if="isStartingAllGames" class="h-3.5 w-3.5 animate-spin" />
                            <Play v-else class="h-3.5 w-3.5" />
                            {{ isStartingAllGames ? t('home.startingAll') : t('home.startAll') }}
                        </button>
                        <button type="button" class="inline-flex items-center gap-1.5 rounded-md bg-red-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-red-500 disabled:cursor-not-allowed disabled:opacity-50" :disabled="!hasRunningGames || isStoppingAllGames || isStartingAllGames" @click.stop="stopAllGames">
                            <LoaderCircle v-if="isStoppingAllGames" class="h-3.5 w-3.5 animate-spin" />
                            <Square v-else class="h-3.5 w-3.5" />
                            {{ isStoppingAllGames ? t('home.stoppingAll') : t('home.stopAll') }}
                        </button>
                    </div>
                </div>
                <div v-if="gameList.length === 0" class="text-gray-500 dark:text-gray-400 text-center py-8">
                    {{ t('home.noGames') }}
                </div>
                <div v-else class="space-y-4">
                    <template v-for="(game, gameIndex) in orderedGameList" :key="game.id">
                    <div v-if="streamSectionStart >= 0 && gameIndex === streamSectionStart" class="pt-4">
                        <div class="border-t border-gray-200 dark:border-gray-700"></div>
                        <h3 class="mt-3 text-sm font-semibold text-gray-700 dark:text-gray-200">
                            {{ t('home.streamQuestTitle') }}
                        </h3>
                        <p class="mt-1 text-xs text-gray-500 dark:text-gray-400">
                            {{ t('home.streamQuestNote') }}
                        </p>
                    </div>
                    <div
                        class="p-3 border border-gray-200 dark:border-gray-700 rounded-lg
                        hover:bg-gray-100 dark:hover:bg-gray-700/50 transition-colors
                        duration-200 ease-in-out"
                        :class="[
                            {
                                'ring-1 ring-violet-500/40 shadow-[0px_0px_8px_2px_#8e51ff50] bg-gray-100 dark:bg-gray-700/40': selectedGame?.uid === game.uid,
                            }
                        ]" @click="selectGame(game)"
                    >
                        <div class="flex justify-between items-center">
                            <div class="flex items-center gap-1">
                                <div class="font-medium text-gray-800 dark:text-white">{{ game.name }}</div>
                                <span v-if="isStreamOnlyGame(game)" class="rounded bg-amber-100 px-1.5 py-0.5 text-[11px] font-semibold text-amber-800 dark:bg-amber-900/40 dark:text-amber-200">
                                    {{ t('home.streamQuestBadge') }}
                                </span>
                                <div class="relative inline-flex items-center">
                                    <div class="w-2 h-2 bg-white absolute rounded-full" style="left: 50%; top: 50%; transform: translate(-50%, -50%)"></div>
                                    <div class="relative inline-block">
                                     <IconVerified class="w-5 h-5 text-indigo-500 dark:text-indigo-400"></IconVerified>
                                    </div>
                                </div>
                            </div>
                            <button @click.stop="removeGameFromList(game)" class="text-red-300 hover:text-red-400"
                                v-if="!game.is_running">
                                {{ t('home.remove') }}
                            </button>
                        </div>
                        <div v-if="game.active_quest_names?.length" class="flex flex-wrap gap-1 mt-1.5">
                            <span
                                v-for="(questName, qIdx) in game.active_quest_names"
                                :key="questName"
                                class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-semibold bg-amber-500/15 text-amber-600 dark:text-amber-400 border border-amber-500/30 shadow-sm"
                                :title="game.active_quest_ids?.[qIdx] ? `Quest ID: ${game.active_quest_ids[qIdx]}` : undefined"
                            >
                                <span>🎯</span>
                                <span class="truncate max-w-[200px]">{{ questName }}</span>
                            </span>
                        </div>
                        <div v-else-if="game.active_quest_ids?.length" class="flex flex-wrap gap-1 mt-1.5">
                            <span
                                v-for="questId in game.active_quest_ids"
                                :key="questId"
                                class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-semibold bg-amber-500/15 text-amber-600 dark:text-amber-400 border border-amber-500/30 shadow-sm"
                            >
                                <span>🎯</span>
                                <span class="font-mono text-[11px]">{{ questId }}</span>
                            </span>
                        </div>
                        <div class="flex space-x-2 mt-2">

                            <div class="text-sm text-green-500 dark:text-green-400" v-if="game.is_running">
                                {{ t('home.running') }}
                            </div>
                        </div>
                    </div>
                    </template>
                </div>
            </div>

            <div class="home-panel bg-white dark:bg-gray-800 p-4 rounded-lg shadow md:sticky md:top-4 self-start" :key="forceRerenderKey">
                <h2 class="text-xl font-bold text-gray-900 dark:text-white mb-4">{{ t('home.gameActions') }}</h2>
                <div class="space-y-4">
                    <div class="text-gray-500 dark:text-gray-400 mb-2 text-sm" v-if="!selectedGame || selectedGame === null">
                        {{ t('home.selectGame') }}
                    </div>

                    <div v-if="selectedGame" class="text-gray-500 dark:text-gray-400 mb-4 text-sm">
                        <strong>{{ t('home.name') }}</strong> {{ selectedGame.name }}<br>
                        <strong>ID:</strong> {{ selectedGame.id }}<br>
                        <template v-if="selectedGame.active_quest_ids?.length">
                            <strong>{{ t('home.questIds') }}</strong>
                            <ul class="list-disc list-inside">
                                <li
                                    v-for="(questId, index) in selectedGame.active_quest_ids"
                                    :key="questId"
                                    class="text-gray-500 dark:text-gray-400"
                                >
                                    <span class="font-mono">{{ questId }}</span>
                                    <span v-if="selectedGame.active_quest_names?.[index]" class="ml-1 text-xs opacity-70">
                                        {{ selectedGame.active_quest_names[index] }}
                                    </span>
                                </li>
                            </ul>
                        </template>
                        <strong v-if="selectedGame.aliases && selectedGame.aliases.length > 0">{{ t('home.aliases') }}</strong>
                        <ul v-if="selectedGame.aliases && selectedGame.aliases.length > 0" class="list-disc list-inside" >
                            <li v-for="alias in selectedGame.aliases" :key="alias"
                                class="text-gray-500 dark:text-gray-400">
                                <span class="font-mono">{{ alias }}</span>
                            </li>
                        </ul>
                    </div>
                    <button @click="handleTestRPC(selectedGame)"
                        class="w-full py-2 rounded-lg bg-gray-600 hover:bg-gray-700 text-white">
                        {{ isConnecting || isConnectedToRPC ? t('home.disconnectGateway') : t('home.testRpc') }}
                    </button>

                    <div class="border-t border-gray-200 dark:border-gray-700 my-4"></div>

                    <GameExecutables v-if="selectedGame" :game="selectedGame"
                        @play="playGame"
                        @stop="stopPlaying"
                        @install_and_play="installAndPlay"
                    />

                </div>

                <div class="border-t border-gray-200 dark:border-gray-700 my-5"></div>

                <div class="mt-6 p-4 border border-gray-200 dark:border-gray-700 rounded-lg">
                    <h3 class="font-medium text-gray-800 dark:text-white mb-2">{{ t('home.status') }}</h3>
                    <div class="text-sm text-gray-500 dark:text-gray-400 mb-2">
                        {{ t('home.statusHelp') }}
                    </div>
                    <div v-if="currentlyPlaying" class="text-gray-500 dark:text-gray-400">
                        {{ t('home.currentlyPlaying') }} <span class="text-green-600"> {{gameList.find(g => g.id ===
                            currentlyPlaying)?.name }}</span>
                    </div>
                    <div v-else class="text-gray-500 dark:text-gray-400">
                        {{ t('home.notPlaying') }}
                    </div>
                </div>

                <div v-if="selectedGame" class="my-4">
                    <h3 class="font-medium text-gray-800 dark:text-white mb-2">{{ t('home.gameInfo') }}</h3>

                </div>
            </div>
        </div>
    </div>
</template>

<style scoped>
@reference "../theme/style.css";

.dialogStyle::backdrop {
    @apply bg-black/70 backdrop-blur-xs;
}

@keyframes rainbowFlow {
    0% {
        background-position: 0% 50%;
    }
    50% {
        background-position: 100% 50%;
    }
    100% {
        background-position: 0% 50%;
    }
}

.rgb-rainbow-text {
    background: linear-gradient(90deg, #ff0055, #ff9900, #33ff00, #00ffff, #9900ff, #ff0055);
    background-size: 300% 300%;
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    animation: rainbowFlow 2.5s ease infinite;
    filter: drop-shadow(0px 0px 12px rgba(255, 255, 255, 0.45));
}
</style>
