import { createGlobalState, tryOnMounted } from '@vueuse/core';
import { computed, shallowRef } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useGlobalState } from '@/composables/state';
import { createSearchPriorityLookup, type SearchPriorityItemLike } from '@/utils/search-ranking';

interface SearchPriorityData {
    updated_at: number;
    sources: string[];
    items: SearchPriorityItemLike[];
}

interface SearchPriorityResponse {
    data: SearchPriorityData;
    refreshed: boolean;
    used_cache: boolean;
    error: string | null;
}

export const useSearchPriority = createGlobalState(() => {
    const { searchPriorityTargetCount } = useGlobalState();
    const priorityData = shallowRef<SearchPriorityData>({
        updated_at: 0,
        sources: [],
        items: [],
    });
    const isLoading = shallowRef(false);
    const isReady = shallowRef(false);
    const refreshed = shallowRef(false);
    const usedCache = shallowRef(false);
    const errorMessage = shallowRef<string | null>(null);
    let pendingTargetCount: number | null = null;

    const priorityLookup = computed(() => createSearchPriorityLookup(priorityData.value.items));

    async function refreshSearchPriority(targetCount = searchPriorityTargetCount.value) {
        if (isLoading.value) {
            pendingTargetCount = targetCount;
            return;
        }

        isLoading.value = true;
        errorMessage.value = null;

        let currentTargetCount = targetCount;
        do {
            pendingTargetCount = null;
            try {
                const response = await invoke<SearchPriorityResponse>('refresh_search_priority', {
                    target_count: currentTargetCount,
                });
                priorityData.value = response.data;
                refreshed.value = response.refreshed;
                usedCache.value = response.used_cache;
                errorMessage.value = response.error;
            } catch (error) {
                errorMessage.value = String(error);
                refreshed.value = false;
                usedCache.value = false;
            } finally {
                isReady.value = true;
            }

            if (pendingTargetCount !== null && pendingTargetCount !== currentTargetCount) {
                currentTargetCount = pendingTargetCount;
                continue;
            }
            break;
        } while (true);

        isLoading.value = false;
    }

    tryOnMounted(() => {
        void refreshSearchPriority();
    });

    return {
        priorityData,
        priorityLookup,
        isLoading,
        isReady,
        refreshed,
        usedCache,
        errorMessage,
        refreshSearchPriority,
    };
});
