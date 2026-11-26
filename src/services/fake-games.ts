import { randomString } from '@/utils/random-string';
import type { Game } from '@/types/types';

export const fakeGames = async () => {
  const module = await import('../assets/gamelist.json');
  const gameListData = module.default;
  return new Promise<Game[]>((resolve) => {
    const games = gameListData.slice(0, 30);
    const sorted = games.sort((a, b) => a.name.localeCompare(b.name));

    resolve(sorted.map((game: Game) => {
        game['uid'] = randomString()
        return game
    }))

  });
}
