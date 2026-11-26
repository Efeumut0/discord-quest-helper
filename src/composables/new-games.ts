import { invoke } from '@tauri-apps/api/core'
import { createGlobalState } from '@vueuse/core'
import { shallowRef } from 'vue'
import type { Game } from '@/types/types'

export interface NewGameItem {
  id: string;
  name: string;
  icon_hash?: string | null;
  executable_count: number;
  detected_at: number;
}

export interface NewGamesState {
  updated_at: number;
  last_checked_at: number;
  known_count: number;
  new_games: NewGameItem[];
}

interface NewGamesSyncResponse {
  state: NewGamesState;
  new_games: NewGameItem[];
}

function emptyState(): NewGamesState {
  return {
    updated_at: 0,
    last_checked_at: 0,
    known_count: 0,
    new_games: [],
  }
}

function toCatalogItem(game: Game) {
  return {
    id: game.id,
    name: game.name,
    icon_hash: game.icon_hash ?? null,
    executable_count: game.executables?.length ?? 0,
  }
}

export const useNewGames = createGlobalState(
  () => {
    const state = shallowRef<NewGamesState>(emptyState())
    const newlyDetectedGames = shallowRef<NewGameItem[]>([])
    const isModalOpen = shallowRef(false)
    const isSyncing = shallowRef(false)
    const errorMessage = shallowRef('')

    async function loadNewGamesState() {
      try {
        state.value = await invoke<NewGamesState>('get_new_games_state')
        errorMessage.value = ''
      } catch (error) {
        errorMessage.value = String(error)
      }
    }

    async function syncNewGames(games: Game[]) {
      if (isSyncing.value || games.length === 0) {
        return;
      }

      isSyncing.value = true
      try {
        const response = await invoke<NewGamesSyncResponse>('sync_new_games', {
          games: games.map(toCatalogItem),
        })
        state.value = response.state
        if (response.new_games.length > 0) {
          newlyDetectedGames.value = response.new_games
          isModalOpen.value = true
        }
        errorMessage.value = ''
      } catch (error) {
        errorMessage.value = String(error)
      } finally {
        isSyncing.value = false
      }
    }

    async function clearNewGames() {
      try {
        state.value = await invoke<NewGamesState>('clear_new_games')
        newlyDetectedGames.value = []
        isModalOpen.value = false
        errorMessage.value = ''
      } catch (error) {
        errorMessage.value = String(error)
      }
    }

    function closeNewGamesModal() {
      isModalOpen.value = false
    }

    return {
      state,
      newlyDetectedGames,
      isModalOpen,
      isSyncing,
      errorMessage,
      loadNewGamesState,
      syncNewGames,
      clearNewGames,
      closeNewGamesModal,
    }
  },
)
