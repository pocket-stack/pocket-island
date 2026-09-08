# Repository split

The reusable part landed in [PocketJS PR #384](https://github.com/pocket-stack/pocketjs/pull/384),
main commit **`627d0eeeb62e79e6eebdebeccc754e151ef7fd31`**.

The application was migrated from PocketJS `engine/pocket3d/examples/island`
at commit **`3f574060e23bcce08a1512d120e871020b3b5566`**. Source assets,
18 authored animation/expression clips and historical evidence are retained.
Historical receipts keep their original build IDs and file paths; they do not
claim to measure a binary built from this repository.

## Dependency direction

`pocket-island` depends on the exact `vendor/pocketjs` Git revision. PocketJS
does not import this application or include its resources in the engine
workspace, npm package or generic CI. Reusable changes are reviewed upstream;
the app updates its submodule after those changes reach `main`.

| PocketJS / Pocket3D | Pocket Island |
| --- | --- |
| TRS sampling, skeleton hierarchy and pose interpolation (`pocket3d-anim`); shared skin bindings and P3M1 decoding (`pocket3d-mesh`) | Clip names, gait phase, transition timing and expression visibility |
| Resident indexed PICA geometry, palette upload, colored-mesh lighting mechanism | Island sun parameters, terrain, shadows and character assets |
| Paired TCP framing, authentication and screenshot transport | `island.*` commands, JS candidate validation/persistence, benchmark workloads |
| 3DS native compiler and QuickJS archive support | Executable lifecycle, C ABI, keyboard/UI, input mapping, orbit and speech layout |

The independent Pocket3D folding-prop fixture tests two rig sizes, shared
geometry, hidden joints and multiple lighting configurations without Island
resources. This application's own tests retain movement, foot contact, sitting,
expressions, chat admission, interpolation and rendered-frame checks.

## Upgrading the engine

Make the generic change in `pocket-stack/pocketjs`, validate it and merge its
PR. In this repository, fetch that revision inside `vendor/pocketjs`, check it
out and commit the updated gitlink. Keep both Cargo lockfiles reviewed. Run:

```sh
bun island test
bun island capture
bun island e2e
bun island build
ISLAND_LINK_E2E=1 bun island e2e
```

The native `3ds/core` manifest remains a separate Rust build for the ARM target;
it consumes this repository's app library and the same submodule animation/mesh crates.
JavaScript-only edits retain `bun island push` and `bun island dev`. A native
or embedded-asset change requires an updated `.3dsx`.

[Asset and GPU parity receipts](../evidence/split-parity.json) verify seven
unchanged asset/layout inputs and eleven upper-screen action frames. The
maximum color-channel difference is one byte level after lighting became
caller-provided. Ten lower-screen frames are byte-identical; the debug panel
with its changed build label is excluded.

## Shared mesh and pose refactor

[PocketJS PR #385](https://github.com/pocket-stack/pocketjs/pull/385), main
commit **`5667d3778a5d3f2ba6f58574d93274435faed7af`**, moves mesh
ownership into `pocket3d-mesh`. Desktop glTF and handheld P3M1 use the same
`Skin` binding and joint-palette evaluator. CPU and GPU preparation share the
colored profile's visibility rule. Island consumes the shared pose interpolator
for clip transitions and display frames; expression visibility remains app policy.

[Pixel comparison](../evidence/shared-mesh-parity.json) matches eleven upper
frames and ten regular lower frames byte-for-byte against the previous main
build. The debug lower screen is excluded because its build label changes.
[Runtime evidence](../evidence/shared-mesh-live.json) covers the native connection,
script replacement/rejection, camera, chat and crowd paths under Azahar.
These receipts identify their build/source inputs and do not measure hardware FPS.
