/** Hardware evidence for two paired running Island instances. Inputs here are
 * scripted; the resulting report is not a human-control acceptance receipt. */
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { PocketRuntimeClient, parsePocketRuntimeToken } from "../vendor/pocketjs/tools/3ds-runtime-client.ts";
const root = resolve(import.meta.dir, "..");
const hosts = process.argv.slice(2);
if (hosts.length !== 2 || hosts[0] === hosts[1]) throw new Error("Usage: bun island multiplayer-probe <first-device-ip> <second-device-ip>");
const out = `${root}/dist/multiplayer/probe-${Date.now()}`;
mkdirSync(out, { recursive: true });
const clients = hosts.map(host => new PocketRuntimeClient({ host, token: parsePocketRuntimeToken(readFileSync(`${root}/.pocket/3ds/devices/${host}-8131.key`, "utf8")), timeoutMs: 15000 }));
let sequence = 0;
async function rpc(peer: number, t: string, fields: Record<string, unknown> = {}, response = "island.stats") {
  const id = `room-probe-${++sequence}`;
  const done = clients[peer].waitForCtrl(m => m.t === response && m.id === id, 15000);
  await clients[peer].sendCtrl({ t, id, ...fields });
  return await done;
}
const samples: Record<string, unknown>[] = [];
function save(extra: Record<string, unknown> = {}) {
  writeFileSync(`${out}/report.json`, JSON.stringify({ hosts, observedAt: new Date().toISOString(), input: "scripted cross-device movement", samples, ...extra }, null, 2));
}
async function sample() {
  const pair = await Promise.all(clients.map((_, i) => rpc(i, "island.stats")));
  samples.push({ at: new Date().toISOString(), players: pair }); save();
  for (let i = 0; i < 2; i++) {
    if (pair[i].online !== 1 || pair[i].remoteOnline !== 1 || pair[i].actorCount !== 2 || pair[i].benchmarkEnabled !== 0) throw new Error(`Player ${i+1} is not rendering the connected room`);
    if (pair[i].playerId === pair[1-i].playerId) throw new Error("Duplicate room identity");
  }
  return pair;
}
async function move(peer: number, x: number, z: number, flags = 0) {
  const id = `walk-${++sequence}`;
  const done = clients[peer].waitForCtrl(m => m.t === "island.reply" && m.id === id, 15000);
  const completed = clients[peer].waitForCtrl(m => m.t === "island.stats" && m.id === id, 15000);
  await clients[peer].sendCtrl({ t: "island.input", id, x, z, flags, frames: 90 });
  const reply = await done;
  if (reply.ok !== true) throw new Error(`Movement rejected: ${reply.message}`);
  await completed;
}
try {
  const connected = await Promise.all(clients.map(c => c.connect()));
  if (connected.some(c => c.hostAbi !== 0)) throw new Error("Expected native Island runtimes");
  const before = await sample();
  // Short opposing Z movement stays near the plaza and in the close camera.
  await Promise.all([move(0, 0, -.4), move(1, 0, .4)]);
  await Bun.sleep(700);
  const moved = await sample();
  for (let i = 0; i < 2; i++) {
    const travelled = Math.hypot(Number(moved[i].x)-Number(before[i].x), Number(moved[i].z)-Number(before[i].z));
    const error = Math.hypot(Number(moved[i].remoteX)-Number(moved[1-i].x), Number(moved[i].remoteZ)-Number(moved[1-i].z));
    if (travelled < .2 || error > .05) throw new Error(`Cross-device movement mismatch: player ${i+1}, moved ${travelled}, remote error ${error}`);
  }
  await Promise.all([move(0, 0, .4), move(1, 0, -.4)]);
  await Bun.sleep(2200); // Exclude startup and movement-command round trips.
  const windows: Array<Record<string, any>[]> = [[], []];
  const deadline = Date.now()+45000;
  while (windows.some(w => w.length < 20) && Date.now() < deadline) {
    const pair = await sample();
    for (let i=0; i<2; i++) {
      const row = pair[i];
      if (Number(row.samples)>0 && !windows[i].some(s => s.elapsedMs === row.elapsedMs)) windows[i].push(row);
    }
    if (windows.some(w => w.length < 20)) await Bun.sleep(500);
  }
  const timing = windows.map((rows, i) => {
    if (rows.length < 20) throw new Error(`Missing timing windows for ${hosts[i]}`);
    const frames = rows.reduce((n, r) => n + r.samples, 0);
    const ms = rows.reduce((n, r) => n + r.frameMs*r.samples, 0) / frames;
    return { host: hosts[i], windows: rows.length, frames, fps: 1000/ms, minWindowFps: Math.min(...rows.map(r => r.fps)), maxFrameMs: Math.max(...rows.map(r => r.maxMs)), maxP95Ms: Math.max(...rows.map(r => r.p95Ms)), rows };
  });
  for (let i=0; i<2; i++) {
    const shot = clients[i].waitForScreenshot(30000);
    await clients[i].sendCtrl({ t: "screenshot" });
    writeFileSync(`${out}/player-${i+1}.png`, (await shot).png);
  }
  save({ passed: true, before, moved, timing });
  console.log(JSON.stringify({ report: out, timing: timing.map(({ rows, ...rest }) => rest) }, null, 2));
} catch (error) { save({ passed: false, error: String(error) }); throw error; }
finally { clients.forEach(c => c.close()); }
