import { resolve } from "node:path";
const root = resolve(import.meta.dir, "..");
for (const command of [
  ["git", "submodule", "update", "--init", "--recursive"],
  ["rustup", "toolchain", "install", "nightly-2026-07-02", "--profile", "minimal", "--component", "rust-src"],
]) {
  const child = Bun.spawn(command, { cwd: root, stdout: "inherit", stderr: "inherit" });
  if (await child.exited) throw new Error(`Setup failed: ${command[0]}`);
}
// Import after submodule initialization so a non-recursive clone can run setup.
const { ensureQuickJsSources } = await import("../vendor/pocketjs/tools/3ds-toolchain.ts");
ensureQuickJsSources();
console.log("Pinned engine and Rust/QuickJS sources are ready. Start Docker, then bun island build.");
