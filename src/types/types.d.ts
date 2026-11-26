
export interface GameExecutable {
  is_launcher: boolean;
  name: string;
  os: string;
  filename?: string;
  path?: string;
  segments?: number;
  is_running?: boolean;
  is_installed?: boolean;
}
export interface Game {
    uid?: string;
    id: string;
    name: string;
    icon_hash?: string;
    executables: GameExecutable[];
    aliases?: string[];
    themes?: string[];
    third_party_skus?: { distributor?: string; id?: string }[];
    active_quest_ids?: string[];
    active_quest_names?: string[];
    is_running?: boolean;
    is_installed?: boolean;
}

export interface GameActionsProvider {
  canPlayGame: (game: Game | null) => boolean;
  isGameInstalled: (game: Game | null) => boolean;
  isExecutableRunning: (executable: GameExecutable) => boolean;
  isGameExecutableInstalled: (executable: GameExecutable) => boolean;
}
