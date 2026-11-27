import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { message } from '@tauri-apps/plugin-dialog';
import { useGlobalState } from './state';

interface RunningGame {
    app_id: string;
    path: string;
    executable_name: string;
}

export function useCloseGames() {
    const { closeGameBehavior, t, addLog } = useGlobalState();
    let unlisten: (() => void) | undefined;
    let disposed = false;
    let handlingClose = false;

    async function start() {
        const window = getCurrentWindow();
        try {
            const stopListening = await window.onCloseRequested(async (event) => {
                if (handlingClose) {
                    event.preventDefault();
                    return;
                }
                const behavior = closeGameBehavior.value;
                if (behavior === 'leave') return;

                event.preventDefault();
                handlingClose = true;
                try {
                    const games = await invoke<RunningGame[]>('get_running_generated_game_processes');
                    if (games.length > 0) {
                        let stopGames = behavior === 'stop';
                        if (behavior === 'ask') {
                            const count = new Set(games.map(game => game.app_id)).size;
                            const stop = t('closeGames.stop');
                            const leave = t('closeGames.leave');
                            const answer = await message(
                                t(count === 1 ? 'closeGames.questionOne' : 'closeGames.question').replace('{count}', String(count)),
                                {
                                    title: t('settings.closeGameBehavior'),
                                    kind: 'warning',
                                    buttons: { yes: stop, no: leave, cancel: t('closeGames.cancel') },
                                },
                            );
                            if (answer === stop || answer === 'Yes') stopGames = true;
                            else if (answer !== leave && answer !== 'No') return;
                        }

                        if (stopGames) {
                            const results = await Promise.allSettled(games.map(game => invoke('stop_process', {
                                app_id: game.app_id,
                                path: game.path,
                                exec_name: game.executable_name,
                            })));
                            const failure = results.find(result => result.status === 'rejected');
                            if (failure) throw failure.reason;
                        }
                    }
                    await window.destroy();
                } catch (error) {
                    addLog('error', String(error));
                    await message(t('closeGames.failed'), { kind: 'error' }).catch(() => undefined);
                } finally {
                    handlingClose = false;
                }
            });
            if (disposed) stopListening();
            else unlisten = stopListening;
        } catch (error) {
            addLog('error', String(error));
        }
    }

    function dispose() {
        disposed = true;
        unlisten?.();
    }

    return { start, dispose };
}
