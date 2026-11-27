import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import ts from 'typescript';
const source = readFileSync(new URL('../src/utils/game-executables.ts', import.meta.url), 'utf8');
const code = ts.transpileModule(source.replace("import { EXECUTABLE_OS } from '@/constants/constants';", "const EXECUTABLE_OS = { WINDOWS: 'win32', LINUX: 'linux', DARWIN: 'darwin' };"), { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
const exports = {};
new Function('exports', code)(exports);
const { getPlayableExecutables, getSourceExecutables } = exports;
const exe = (name, path = '', extra = {}) => ({ name, path, os: 'win32', is_launcher: false, ...extra });
const game = { id: '123', name: 'GameName', executables: [] };
assert.deepEqual(getPlayableExecutables(game).map(e => e.filename), ['GameName.exe', 'GameName-Win64-Shipping.exe']);
assert.ok(getPlayableExecutables(game).every(e => e.path === '2.Source' && !e.is_app_id_fallback));
const populated = { ...game, executables: [exe('real.exe', 'bin'), exe('old.exe', '2.OpenAsar'), exe('steam.exe', '3.Steam'), exe('legacy.exe', '', { is_app_id_fallback: true }), exe('GameName.exe', '2.Source', { is_running: true, is_installed: true })] };
assert.deepEqual(getSourceExecutables(populated, 1).map(e => [e.filename, e.path]), [['real.exe', '1.Source/bin']]);
assert.equal(getSourceExecutables(populated, 2)[0].is_running, true);
assert.equal(getSourceExecutables(populated, 2)[0].is_installed, true);
assert.equal(getSourceExecutables({ ...game, executables: [exe('real.exe', '1.Source/bin')] }, 1)[0].path, '1.Source/bin');
const home = readFileSync(new URL('../src/pages/HomeView.vue', import.meta.url), 'utf8');
assert.doesNotMatch(home, /consoleMode|ConsoleScript|homeSealStatus|get_package_seal_status|requires_risk_acceptance|allow_custom_rpc_link|createAppIdFallbackExecutable/);
const normalLaunch = home.slice(home.indexOf('async function createDummyGame'), home.indexOf('async function handleTestRPC'));
assert.match(normalLaunch, /invoke\('create_fake_game', payload\)/);
assert.match(normalLaunch, /invoke\('run_background_process', payload\)/);
assert.doesNotMatch(normalLaunch, /connect_to_discord_rpc_3|handleTestRPC/);
const component = readFileSync(new URL('../src/components/GameExecutables.vue', import.meta.url), 'utf8');
assert.doesNotMatch(component, /option value="[34]"|source[34]Executables|is_app_id_fallback|appIdMethod/);
console.log('Game source cleanup regression checks passed.');

// Execute GamesView's actual function bodies, with only its external dependencies mocked.
const gamesView = readFileSync(new URL('../src/pages/GamesView.vue', import.meta.url), 'utf8');
const script = gamesView.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1];
const ast = ts.createSourceFile('GamesView.ts', script, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
const functions = ast.statements.filter(node => ts.isFunctionDeclaration(node) && ['createGameFiles', 'formatMessage'].includes(node.name?.text)).map(node => node.getText(ast)).join('\n');
const functionCode = ts.transpileModule(functions, { compilerOptions: { target: ts.ScriptTarget.ES2020 } }).outputText;
const calls = [];
const logs = [];
const statusMessage = { value: '' };
const creatingGameId = { value: null };
let failCreation = false;
const createGameFiles = new Function('getPlayableExecutables', 'getExecutableLaunchPayload', 'invoke', 'statusMessage', 'creatingGameId', 't', 'addLog', `${functionCode}; return createGameFiles;`)(
    getPlayableExecutables, exports.getExecutableLaunchPayload,
    async (command, payload) => {
        calls.push({ command, payload });
        assert.equal(creatingGameId.value, '123');
        if (failCreation) throw new Error('creation failed');
    },
    statusMessage, creatingGameId,
    key => key === 'games.statusCreated' ? '{name}: {count}' : key,
    (...args) => logs.push(args),
);
await createGameFiles(game);
assert.deepEqual(calls.map(call => call.payload), [
    { name: 'GameName', path: '2.Source', executable_name: 'GameName.exe', app_id: '123', icon_hash: '' },
    { name: 'GameName', path: '2.Source', executable_name: 'GameName-Win64-Shipping.exe', app_id: '123', icon_hash: '' },
]);
assert.ok(calls.every(call => call.command === 'create_fake_game'));
assert.equal(statusMessage.value, 'GameName: 2');
assert.equal(creatingGameId.value, null);
calls.length = 0;
await createGameFiles({ ...game, icon_hash: 'icon', executables: [exe('Binaries/Win64/real.exe')] });
assert.deepEqual(calls[0].payload, { name: 'GameName', path: '1.Source/Binaries/Win64', executable_name: 'real.exe', app_id: '123', icon_hash: 'icon' });
assert.ok(calls.slice(1).every(call => call.payload.path === '2.Source'));
calls.length = 0;
await createGameFiles({ ...game, executables: [exe('Binaries\\Win64\\real.exe', '1.Source/Binaries/Win64')] });
assert.equal(calls[0].payload.path, '1.Source/Binaries/Win64');
assert.equal(calls[0].payload.executable_name, 'real.exe');
assert.equal(exports.getExecutableLaunchPayload(exe('nested/real.exe', '4.Source/custom')).path, '4.Source/custom');
const processCandidates = exports.getProcessCandidateExecutables({ ...game, executables: [exe('Binaries/real.exe')] });
assert.ok(processCandidates.some(e => e.path === 'Binaries' && e.filename === 'real.exe'));
assert.ok(processCandidates.some(e => e.path === '1.Source/Binaries' && e.filename === 'real.exe'));
assert.ok(getPlayableExecutables({ ...game, executables: [exe('Binaries/real.exe')] }).every(e => e.path.startsWith('1.Source') || e.path === '2.Source'));
assert.ok(exports.getProcessCandidateExecutables(populated).every(e => !/^(?:2\.OpenAsar|3\.Steam)(?:[\\/]|$)/.test(e.path)));
assert.equal(exports.isSupportedExecutable(exe('legacy.exe', '', { is_app_id_fallback: true })), false);
failCreation = true;
calls.length = 0;
await createGameFiles(game);
assert.equal(calls.length, 1);
assert.match(statusMessage.value, /creation failed/);
assert.equal(logs.at(-1)[0], 'error');
assert.equal(creatingGameId.value, null);
assert.doesNotMatch(readFileSync(new URL('../src/types/types.d.ts', import.meta.url), 'utf8'), /is_app_id_fallback/);
console.log('GamesView creation, errors, nested paths, and legacy process behavior checks passed.');

const homeScript = home.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1];
const homeAst = ts.createSourceFile('HomeView.ts', homeScript, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
const startAllNode = homeAst.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === 'startAllGames');
const startAllCode = ts.transpileModule(startAllNode.getText(homeAst), { compilerOptions: { target: ts.ScriptTarget.ES2020 } }).outputText;
let starts = 0;
let refreshes = 0;
const starting = { value: false };
const streamOnlyNode = homeAst.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === 'isStreamOnlyGame');
const streamOnlyCode = ts.transpileModule(streamOnlyNode.getText(homeAst), { compilerOptions: { target: ts.ScriptTarget.ES2020 } }).outputText;
const isStreamOnlyGame = new Function(`${streamOnlyCode}; return isStreamOnlyGame;`)();
assert.equal(isStreamOnlyGame({ has_stream_quest: true }), true);
assert.equal(isStreamOnlyGame({ has_stream_quest: true, has_play_quest: true }), false, 'a quest that also asks for play stays playable');
assert.equal(isStreamOnlyGame({ has_play_quest: true }), false);
assert.equal(isStreamOnlyGame({}), false, 'a game added by hand is never treated as stream-only');

function buildStartAll(games, candidates) {
    return new Function('isStartingAllGames', 'isStoppingAllGames', 'gameList', 'isStreamOnlyGame', 'getAllCandidateExecutables', 'isExecutableRunning', 'getExecutableLaunchPayload', 'playGame', 'installAndPlay', 'refreshGameProcessStates', 'addLog', 'errorMessage', `${startAllCode}; return startAllGames;`)(
        starting, { value: false }, { value: games }, isStreamOnlyGame,
        () => candidates,
        () => false, exports.getExecutableLaunchPayload,
        async () => { starts++; }, async () => { starts++; }, async () => { refreshes++; },
        () => {}, String,
    );
}

const startAllGames = buildStartAll([game], [{ ...exe('real.exe', '1.Source'), is_running: false }, { ...exe('real.exe'), is_running: true }]);
await startAllGames();
assert.equal(starts, 0, 'an existing base-path process must prevent a second launch');
assert.equal(starting.value, false);
assert.equal(refreshes, 1);
console.log('HomeView start-all respects running legacy base processes.');

starts = 0;
refreshes = 0;
const streamOnlyGame = { ...game, has_stream_quest: true, has_play_quest: false };
const startAllStreamOnly = buildStartAll([streamOnlyGame], [{ ...exe('real.exe', '1.Source'), is_running: false }]);
await startAllStreamOnly();
assert.equal(starts, 0, 'start-all must leave stream-only quest games alone');

starts = 0;
refreshes = 0;
const bothKindsGame = { ...game, has_stream_quest: true, has_play_quest: true };
const startAllBothKinds = buildStartAll([bothKindsGame], [{ ...exe('real.exe', '1.Source'), is_running: false }]);
await startAllBothKinds();
assert.equal(starts, 1, 'a game that also carries a play quest must still start');
console.log('HomeView start-all skips stream-only quest games.');
