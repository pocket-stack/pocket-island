/** Two-device room. The shared companion session owns paired TCP; the Rust
 * authority owns all movement, command time, snapshots and room epochs. */
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { isIP } from "node:net";
import { resolve } from "node:path";
import { connectCompanionSession } from "../vendor/pocketjs/tools/companion-session.ts";
const root = resolve(import.meta.dir, "..");
const endpoints = process.argv.slice(2).map(value => {
  const [address, portText, extra] = value.split(":");
  const port = portText === undefined ? 8741 : Number(portText);
  if (isIP(address) !== 4 || extra !== undefined || !Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error("Expected an IPv4 address with an optional port");
  }
  return { address, port };
});
if (endpoints.length !== 2 || new Set(endpoints.map(e => `${e.address}:${e.port}`)).size !== 2) {
  throw new Error("Usage: bun island companion <first-ip[:port]> <second-ip[:port]>");
}
const build = Bun.spawnSync(["cargo", "build", "--locked", "--release", "--bin", "companion"], { cwd: root, stdout: "inherit", stderr: "inherit" });
if (build.exitCode) throw new Error("Authority build failed");
const keys = endpoints.map(e => readFileSync(resolve(root, `.pocket/3ds/devices/${e.address}-${e.port}.key`), "utf8").trim());
const authority = spawn(resolve(root, "target/release/companion"), [String(Date.now())], { stdio: ["pipe", "pipe", "inherit"] });
let stopped = false, tickPending = false, tickAt = 0, output = "";
function write(line: string) {
  if (stopped || authority.stdin.destroyed || authority.stdin.writableLength > 8192) return false;
  authority.stdin.write(`${line}\n`); return true;
}
const sessions = endpoints.map(({ address, port }, peer) => connectCompanionSession({
  address, port, key: keys[peer],
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
    else if (type === "P" && typeof record === "string" && record.length <= 512) {
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
