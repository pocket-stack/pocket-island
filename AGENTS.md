# Repository instructions

- Pocket3D owns reusable animation, mesh formats and GPU backends in the pinned `vendor/pocketjs` submodule. Put Island movement, expressions, chat, camera, scene composition and application lifecycle under this repository.
- Do not edit the submodule to fix application behavior. Send reusable changes to PocketJS, then update the pinned revision here.
- Use Conventional Commits for commits and pull requests. Publish validated changes as a Draft PR before review; mark it ready before an authorized merge.
- Preserve the distinction between native build, emulator rendering, hardware timing and physical interaction evidence. Emulator FPS is not a hardware performance claim.
