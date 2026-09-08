import { resolve } from "node:path";
const root = resolve(import.meta.dir, "..");
for (const command of [
  ["git", "submodule", "update", "--init", "--recursive"],
  ["rustup", "toolchain", "install", "nightly-2026-07-02", "--profile", "minimal", "--component", "rust-src"],
  ["cargo", "fetch", "--locked", "--manifest-path", "vendor/pocketjs/hosts/psp/Cargo.toml"],
]) {
  const child = Bun.spawn(command, { cwd: root, stdout: "inherit", stderr: "inherit" });
  if (await child.exited) throw new Error(`Setup failed: ${command[0]}`);
}
console.log("Pinned engine and Rust/QuickJS sources are ready. Start Docker, then bun build.");
