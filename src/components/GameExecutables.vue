<script setup lang="ts">
import { GameActionsKey } from '@/constants/constants';
import type { Game, GameExecutable } from '@/types/types';
import { computed, inject, ref, watch } from 'vue';
import { useGlobalState } from '@/composables/state';
import {
    getExecutableLaunchPayload,
    getSourceExecutables,
    type SourcedExecutable,
    splitExecutableName,
} from '@/utils/game-executables';

const props = defineProps<{
    game: Game
}>();

const emit = defineEmits<{
    play: [{game: Game, executable: GameExecutable}]
    stop: [{game: Game, executable: GameExecutable}]
    install_and_play: [{game: Game, executable: GameExecutable}]
}>();

const gameActions = inject(GameActionsKey);
const { t } = useGlobalState();

const selectedSource = ref<'auto' | '1' | '2'>('auto');

watch(() => props.game?.uid ?? props.game?.id, () => {
    selectedSource.value = 'auto';
});

const source1Executables = computed(() => getSourceExecutables(props.game, 1));
const source2Executables = computed(() => getSourceExecutables(props.game, 2));
const currentExecutables = computed<SourcedExecutable[]>(() => {
    if (selectedSource.value === '1') return source1Executables.value;
    if (selectedSource.value === '2') return source2Executables.value;
    if (source1Executables.value.some(isExecutableActiveRunning)) return source1Executables.value;
    if (source2Executables.value.some(isExecutableActiveRunning)) return source2Executables.value;
    return source1Executables.value.length ? source1Executables.value : source2Executables.value;
});
const shouldShowGeneratedWarning = computed(() => currentExecutables.value.some(e => e.sourceNumber === 2));

function isExecutableActiveRunning(executable: GameExecutable): boolean {
    const launchPayload = getExecutableLaunchPayload(executable);
    if (gameActions?.isExecutableRunning(launchPayload)) {
        return true;
    }
    const existing = props.game.executables?.find(e =>
        (e.path ?? '').toLowerCase() === (launchPayload.path ?? '').toLowerCase() &&
        (e.filename ?? e.name).toLowerCase() === (launchPayload.filename ?? launchPayload.name).toLowerCase()
    );
    return existing?.is_running ?? executable.is_running ?? false;
}

function isExecutableActiveInstalled(executable: GameExecutable): boolean {
    const launchPayload = getExecutableLaunchPayload(executable);
    if (gameActions?.isGameExecutableInstalled(launchPayload)) {
        return true;
    }
    const existing = props.game.executables?.find(e =>
        (e.path ?? '').toLowerCase() === (launchPayload.path ?? '').toLowerCase() &&
        (e.filename ?? e.name).toLowerCase() === (launchPayload.filename ?? launchPayload.name).toLowerCase()
    );
    return existing?.is_installed ?? executable.is_installed ?? false;
}

function getSourceBadgeText(executable: SourcedExecutable): string {
    if (executable.sourceNumber === 2) {
        return t('sources.source2');
    }
    return `${executable.sourceNumber}. Kaynak`;
}

function getSourceTooltip(executable: SourcedExecutable): string {
    if (executable.sourceNumber === 2) {
        return 'Discord RPC ile profil görünümü; oyun algılaması veya görev ilerlemesi değildir.';
    }
    if (executable.sourceNumber === 1) return t('sources.source1');
    return `${executable.sourceNumber}. Kaynak`;
}

function getButtonText(executable: GameExecutable) {
    if (isExecutableActiveRunning(executable)) {
        return t('executables.stop');
    }

    if (!isExecutableActiveInstalled(executable)) {
        return t('executables.createAndPlay');
    }

    return t('executables.play');
}

function handleLaunch(executable: GameExecutable) {
    const launchExecutable = getExecutableLaunchPayload(executable);

    if (isExecutableActiveRunning(executable)) {
        emit('stop', {
            game: props.game,
            executable: launchExecutable,
        });
    } else {
        if (!isExecutableActiveInstalled(executable)) {
            emit('install_and_play', {
                game: props.game,
                executable: launchExecutable,
            });
        } else {
            emit('play', {
                game: props.game,
                executable: launchExecutable,
            });
        }
    }
}
</script>

<template>
    <div class="text-gray-500 dark:text-gray-400">

        <div class="mb-3">
            <label class="block text-xs font-semibold text-gray-700 dark:text-gray-300 mb-1">
                {{ t('sources.title') }}
            </label>
            <div class="relative">
                <select
                    v-model="selectedSource"
                    class="w-full bg-gray-50 dark:bg-gray-700/80 border border-gray-300 dark:border-gray-600 rounded-lg px-3 py-2 text-xs font-medium text-gray-900 dark:text-white focus:outline-none focus:ring-2 focus:ring-violet-500 shadow-sm cursor-pointer transition-colors"
                >
                    <option value="auto">{{ t('sources.auto') }}</option>
                    <option value="1">{{ t('sources.source1') }}</option>
                    <option value="2">{{ t('sources.source2') }}</option>
                </select>
            </div>
        </div>

        <div v-if="currentExecutables.length === 0" class="space-y-3 my-3">
            <div class="rounded-lg border border-amber-300/80 bg-amber-50/80 dark:border-amber-700/60 dark:bg-amber-950/40 p-3 text-xs text-amber-900 dark:text-amber-200 text-center font-medium shadow-sm">
                {{ t('sources.notFoundInSource') }}
            </div>
        </div>

        <div v-else class="text-xs mt-2">
            <div v-for="(executable) in currentExecutables" :key="`${executable.sourceNumber}-${executable.path}-${executable.name}`"
                class="grid grid-cols-[auto_1fr_auto] gap-2 items-center mb-2 w-full">

                <div class="w-24 max-w-[100px]">
                    <div
                        class="bg-indigo-100 dark:bg-indigo-950/60 border border-indigo-300 dark:border-indigo-800 text-indigo-900 dark:text-indigo-200 rounded-full px-2 py-0.5 w-fit text-[11px] font-semibold cursor-help transition-all hover:scale-105 shadow-sm"
                        :title="getSourceTooltip(executable)"
                    >
                        {{ getSourceBadgeText(executable) }}
                    </div>
                </div>

                <div class="relative overflow-hidden">
                    <div class="flex flex-nowrap overflow-x-auto scrollbar-none max-w-full pr-4 fade-right">
                        <div v-for="(section, i) in splitExecutableName(executable)" :key="i"
                            class="text-center border border-gray-300 dark:border-gray-700 rounded-md px-2 py-1 mr-1 whitespace-nowrap">
                            <span>{{ section }}</span>
                        </div>
                    </div>
                </div>

                <div class="justify-self-end">
                    <button class="text-white rounded-md px-3 py-1 font-medium transition active:scale-95"
                    :class="[
                        {
                            'bg-blue-500 hover:bg-blue-600': !isExecutableActiveRunning(executable),
                            'bg-red-500 hover:bg-red-600': isExecutableActiveRunning(executable),
                        },
                    ]"
                        @click="handleLaunch(executable)"
                    >
                        {{ getButtonText(executable) }}
                    </button>
                </div>
            </div>
        </div>

        <div v-if="shouldShowGeneratedWarning" class="mt-3 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-xs text-amber-900 dark:border-amber-700 dark:bg-amber-950/40 dark:text-amber-100">
            Bu dosya adları oyun adından tahmin edilir. Bu kaynak Discord RPC ile profil görünümü gönderir. Discord’un gerçek oyun algılaması
            veya görev ilerlemesi anlamına gelmez. Bağlantı durumu açılan oyun penceresinde gösterilir.
        </div>
    </div>
</template>

<style scoped>
.fade-right {
    -webkit-mask-image: linear-gradient(to right, black 85%, transparent 100%);
    mask-image: linear-gradient(to right, black 85%, transparent 100%);
}

.scrollbar-none {
    scrollbar-width: none;
    -ms-overflow-style: none;
}

.scrollbar-none::-webkit-scrollbar {
    display: none;
}
</style>
