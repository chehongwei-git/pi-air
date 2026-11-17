/** 真实 Pi 进程池协议验证：临时项目/配置，无模型请求；传入 Pi 的 CLI JS 入口。 */
import { spawn } from 'node:child_process';
import {
  mkdtemp,
  mkdir,
  writeFile,
  readFile,
  rename,
  rm,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { createInterface } from 'node:readline';
import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';

const cli = process.argv[2];
if (!cli)
  throw new Error(
    '用法：node scripts/verify-process-pool.mjs <Pi CLI JS 路径>',
  );
const root = await mkdtemp(join(tmpdir(), 'pi-pool-'));
const pending = new Map();
let serial = 0;
let child;
try {
  const agent = join(root, 'agent');
  const a = join(root, '项目 A');
  const b = join(root, '项目 B');
  await Promise.all([agent, a, b].map((path) => mkdir(path)));
  const sessions = [];
  for (const [index, cwd] of [a, b].entries()) {
    await writeFile(join(cwd, 'marker.txt'), String(index));
    const path = join(root, `session-${index}.jsonl`);
    await writeFile(
      path,
      JSON.stringify({
        type: 'session',
        version: 3,
        id: randomUUID(),
        timestamp: new Date().toISOString(),
        cwd,
      }) + '\n',
    );
    sessions.push(path);
  }
  const started = performance.now();
  child = spawn(
    process.execPath,
    [
      resolve(cli),
      '--mode',
      'rpc',
      '--no-session',
      '--no-extensions',
      '--no-mcp',
      '--no-skills',
      '--no-prompt-templates',
      '--no-themes',
      '--no-context-files',
      '--extension',
      resolve('src-tauri/bridge/desktop.mjs'),
    ],
    {
      cwd: a,
      env: { ...process.env, PI_CODING_AGENT_DIR: agent },
      stdio: ['pipe', 'pipe', 'ignore'],
      windowsHide: true,
    },
  );
  const lines = createInterface({ input: child.stdout });
  lines.on('line', (line) => {
    const event = JSON.parse(line);
    if (event.type === 'response') pending.get(event.id)?.(event);
    // 非交互测试不批准未知扩展交互。
    if (event.type === 'extension_ui_request')
      child.stdin.write(
        JSON.stringify({
          type: 'extension_ui_response',
          id: event.id,
          cancelled: true,
        }) + '\n',
      );
  });
  const request = (type, fields = {}) =>
    new Promise((resolveResult, reject) => {
      const id = String(++serial);
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`${type} 超时`));
      }, 30000);
      pending.set(id, (event) => {
        clearTimeout(timer);
        pending.delete(id);
        event.success
          ? resolveResult(event.data)
          : reject(new Error(event.error));
      });
      child.stdin.write(JSON.stringify({ id, type, ...fields }) + '\n');
    });
  await request('get_state');
  const coldMs = Math.round(performance.now() - started);
  const pid = child.pid;
  const switches = [];
  for (const index of [0, 1, 0]) {
    const before = performance.now();
    assert.equal(
      (await request('switch_session', { sessionPath: sessions[index] }))
        .cancelled,
      false,
    );
    assert.equal((await request('get_state')).sessionFile, sessions[index]);
    const commands = (await request('get_commands')).commands;
    assert(commands.some((command) => command.name === 'pi-desktop-models-v1'));
    const result = await request('bash', {
      command: "node -p \"require('fs').readFileSync('marker.txt','utf8')\"",
    });
    assert.equal(result.output.trim(), String(index));
    assert.equal(child.pid, pid);
    switches.push(Math.round(performance.now() - before));
  }
  // 切换离开 B 后重命名其目录，再经桥接恢复原会话，确认真正的工具 cwd。
  const moved = join(root, '项目 B 已改名');
  const originalHeader = (await readFile(sessions[1], 'utf8')).split('\n')[0];
  await rename(b, moved);
  const relocated = await request('prompt', {
    message: `/pi-desktop-relocate-v1 ${JSON.stringify({ path: sessions[1], cwd: moved })}`,
  });
  assert.equal(relocated.disposition, 'handled');
  const relocatedState = await request('get_state');
  assert.equal(relocatedState.sessionFile, sessions[1]);
  assert.equal(
    (await request('bash', { command: 'node -p "process.cwd()"' })).output
      .trim()
      .toLowerCase(),
    resolve(moved).toLowerCase(),
  );
  assert.equal(
    (
      await request('bash', {
        command: "node -p \"require('fs').readFileSync('marker.txt','utf8')\"",
      })
    ).output.trim(),
    '1',
  );
  await request('set_session_name', { name: '目录改名后继续' });
  const resumed = await readFile(sessions[1], 'utf8');
  assert.equal(resumed.split('\n')[0], originalHeader);
  assert(resumed.includes('目录改名后继续'));
  await request('switch_session', { sessionPath: sessions[0] });
  assert.equal((await request('new_session')).cancelled, false);
  const result = await request('bash', {
    command: "node -p \"require('fs').readFileSync('marker.txt','utf8')\"",
  });
  assert.equal(result.output.trim(), '0');
  console.log(
    JSON.stringify({
      coldMs,
      switchAndProbeMs: switches,
      sameProcess: true,
      crossProjectRelativePaths: true,
      bridgeReloaded: true,
      newSessionKeepsProject: true,
      relocatedSessionKeepsFileAndHistory: true,
      relocatedToolsUseNewDirectory: true,
    }),
  );
} finally {
  if (child && child.exitCode === null) {
    child.stdin.end();
    await new Promise((resolveExit) => {
      const timer = setTimeout(() => child.kill(), 3000);
      child.once('exit', () => {
        clearTimeout(timer);
        resolveExit();
      });
    });
  }
  await rm(root, { recursive: true, force: true });
}
