import type { Game } from '@/types/types'

export interface ActiveQuestCatalogItem {
    id: string
    name: string
    icon_hash: string | null
    aliases: string[]
    executables: { name: string; os: string }[]
}

/** The slice of the Discord catalog the backend needs to match quests. */
export function buildActiveQuestCatalog(games: Game[]): ActiveQuestCatalogItem[] {
    return games.map(game => ({
        id: game.id,
        name: game.name,
        icon_hash: game.icon_hash ?? null,
        aliases: game.aliases ?? [],
        executables: (game.executables || []).map(executable => ({
            name: executable.name,
            os: executable.os,
        })),
    }))
}
