/** Two-device room. The shared companion session owns paired TCP; the Rust
 * authority owns all movement, command time, snapshots and room epochs. */
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { connectCompanionSession } from "../vendor/pocketjs/tools/companion-session.ts";
const root = resolve(import.meta.dir, "..");
const addresses = process.argv.slice(2);
if (addresses.length !== 2 || addresses[0] === addresses[1] || addresses.some(a => !/^(\d{1,3}\.){3}\d{1,3}$/.test(a))) {
  throw new Error("Usage: bun island companion <first-device-ip> <second-device-ip>");
}
const build = Bun.spawnSync(["cargo", "build", "--locked", "--release", "--bin", "companion"], { cwd: root, stdout: "inherit", stderr: "inherit" });
if (build.exitCode) throw new Error("Authority build failed");
const keys = addresses.map(a => readFileSync(resolve(root, `.pocket/3ds/devices/${a}-8741.key`), "utf8").trim());
const authority = spawn(resolve(root, "target/release/companion"), [String(Date.now())], { stdio: ["pipe", "pipe", "inherit"] });
let stopped = false, tickPending = false, tickAt = 0, output = "";
function write(line: string) {
  if (stopped || authority.stdin.destroyed || authority.stdin.writableLength > 8192) return false;
  authority.stdin.write(`${line}\n`); return true;
}
const sessions = addresses.map((address, peer) => connectCompanionSession({
  address, key: keys[peer],
  record(record) {
    if (record.length > 512 || !/^[0-9a-f]+$/.test(record) || !write(`P ${peer} ${record}`)) throw new Error("Room input budget");
  },
  connected() { console.log(`Player ${peer+1}: transport ${address}; waiting for paired room hello`); },
  disconnected() { write(`L ${peer}`); console.log(`Player ${peer+1}: disconnected; reconnecting`); },
}));
authority.stdout.setEncoding("utf8");
authority.stdout.on("data", (chunk: string) => {
  output += chunk;
  let end: number;
  while ((end = output.indexOf("\n")) >= 0) {
    const line = output.slice(0, end); output = output.slice(end+1);
    const [type, peerText, record] = line.split(" ");
    if (type === "T") { tickPending = false; continue; }
    const peer = Number(peerText);
    if (!Number.isInteger(peer) || peer < 0 || peer > 1) return stop(1);
    if (type === "D") sessions[peer].disconnect();
    else if (type === "P" && record?.length <= 512) {
      // Full snapshots are replaceable. Missing credit drops this snapshot;
      // hello retries recover a dropped welcome. Commands stay in the client.
      sessions[peer].send(record);
    } else return stop(1);
  }
  if (output.length > 2048) stop(1);
});
const tick = setInterval(() => {
  if (tickPending) {
    if (performance.now() - tickAt > 3000) stop(1);
    return;
  }
  tickPending = write("T"); tickAt = performance.now();
}, 1000/30);
const stats = setInterval(() => write("S"), 1000);
function stop(code = 0) {
  if (stopped) return;
  stopped = true; clearInterval(tick); clearInterval(stats);
  for (const session of sessions) session.close();
  authority.stdin.end(); authority.kill();
  process.exitCode = code;
}
authority.on("error", () => stop(1));
authority.on("exit", code => stop(code ?? 1));
process.on("SIGINT", () => stop()); process.on("SIGTERM", () => stop());
console.log("Pocket Island companion: 30 Hz authority, 15 Hz snapshots, paired players 1 and 2");
