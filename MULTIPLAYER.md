# Two consoles through a companion

The demo synchronizes **two walking avatars, body actions and expressions**
through a paired companion on the same LAN. Each console predicts its own
movement at 30 Hz. The companion confirms input sequences with authoritative
state; a correction restores that state and replays unconfirmed simulation
steps. The other avatar uses a three-tick (100 ms) snapshot buffer and the
existing 60 Hz pose presentation. Missing snapshots hold the newest known
pose. Playback time never moves backwards when an update arrives late.

## Install and run

Keep both consoles in ftpd while installing:

```sh
bun run setup
bun island build
bun island upload 192.168.8.102 192.168.8.152
bun island companion 192.168.8.102 192.168.8.152
```

Exit ftpd on both consoles and launch Pocket Island. The daemon reconnects
until both apps are available. The lower screen shows the assigned player
number and visitor count. Each device controls its own avatar; the other
avatar has a visitor label. Circle Pad moves, B runs, A waves, X sits/stands,
and L/R selects expressions. START leaves the app. The daemon removes a
missing peer and sends its current full state when it rejoins.

The upload command finds the SD card's `3ds` directory regardless of case,
reads and backs up an existing ROM, uploads to a temporary name, verifies the
bytes, and activates the file with a rename. A second read verifies the active
ROM. It preserves existing keys, creates missing application and development
keys, and stores their local copies with mode 0600 under ignored `.pocket/`.
Receipts and prior ROMs go to ignored `dist/multiplayer/`.

The companion accepts an optional `ip:port` endpoint for a forwarded or test
connection; the default port is 8741. It connects to each device on **8741** using the existing PocketJS
offload worker transport. The application key is
`/pocketjs/offload/edc8784e4061e8bd.key`, derived from `pocket-island` by the
framework's app-key convention. The daemon binds its first configured device
to player 1 and the second to player 2. Each device's hello is accepted only
when its simulation fingerprint matches the authority. Run the daemon from
the same checkout used to build the ROMs.

The development connection on **8131** remains available for screenshots,
script replacement and input tapes while the room is connected:

```sh
bun island probe --host 192.168.8.102
bun island probe --host 192.168.8.152
bun island multiplayer-probe 192.168.8.102 192.168.8.152
bun island push --host 192.168.8.102
```

A script replacement preserves the room and simulation. Native C/Rust or
embedded asset changes require another build, upload and application restart.
The local `crowd` benchmark suspends room input and uses its own test actors;
exit the benchmark to rejoin from an authoritative snapshot.

## Ownership and frame boundaries

| Owner | Responsibility |
| --- | --- |
| PocketJS `pocket-sim` | Bounded prediction/checkpoints, reconciliation, ordered command queue, snapshot bracketing |
| PocketJS companion session / offload worker | Pairing, TCP framing, reconnect, bounded queues, thread ownership |
| Island `src/motion.rs` | Collision, speed, action transitions, authored stride/clip clocks |
| Island `src/net.rs` | Avatar schema, room epochs, player assignment, 30 Hz command credit, 100 ms playback delay |
| Island `src/bin/companion.rs` | Authoritative room using the same Rust reducer as the 3DS |
| Island `scripts/companion.ts` | Device configuration and bounded process mailbox; it contains no movement formula |
| Island native host | Hardware input, frame-boundary delivery, render poses, camera, UI and DevTools adapter |

`Motion` is a copyable checkpoint containing position, facing, action,
expression, clip phase, simulation tick and bench state. It contains no mesh,
skeleton pose buffers, camera, chat, UI or network resource. Reconciliation
never runs the outer UI transaction or sends a chat message. Avatar meshes
remain shared and resident in GPU memory; networking changes only the small
pose palettes for the two rendered actors.

The [framework design](vendor/pocketjs/docs/SIMULATION.md) covers how this
relates to PocketJS frame transactions and the primary GGPO, Valve and VRChat
sources. It also describes the boundaries of the reusable native simulation
API. There is no arbitrary JavaScript heap rollback.

## Budgets and recovery

- Client prediction retains **64 inputs/checkpoints**, at most 2.13 seconds at
  30 Hz. Reconciliation is bounded by that history. Full history stalls new
  predictions; it never overwrites an unconfirmed input.
- The authority grants one command step per 30 Hz tick and banks at most three
  credits for jitter. A large packet grants no additional movement time.
- Inputs have contiguous sequence numbers, quantized axes and one-shot action
  bits. Duplicates do not repeat actions. Invalid fields, gaps, wrong epochs,
  incompatible rules and unauthorized player channels are rejected.
- Full snapshots are emitted at **15 Hz**. Remote history holds at most 12
  snapshots. The fixed hex wire schema is bounded to 512 bytes per record;
  the current two-avatar snapshot occupies 196 bytes before transport framing.
- Each display frame takes at most one worker record and submits at most two;
  socket IO and key reads execute on the shared lower-priority worker.
- A peer missing input for three server seconds is removed. A client missing
  snapshots for 180 display frames starts a new hello. Each join establishes
  a new epoch and clears old commands; disconnect/rejoin preserves the
  authority's last avatar state while the daemon remains alive.
- Daemon restart starts a new room. Its process mailbox is bounded; a stalled
  authority is terminated instead of accumulating timer callbacks.

Each player's walking is independent; avatars can pass through one another.
The authority validates Island terrain movement. This is not a combat demo,
a shared-object physics solver, or server-side hit detection.

Conversation entries and speech bubbles remain local and are labeled local.
The existing sender/sequence/delivery model is the boundary for a subsequent
reliable chat channel. Voice, account identity, persistence and internet
transport are outside this two-device walking demo. Pairing uses the existing
LAN key contract; the mailbox does not encrypt traffic.

## Validation

`bun island test` covers the original asset/pose/locomotion tests and the
network protocol through encoded packets. The two-client tape injects jitter,
skips snapshots, applies an authoritative position correction, verifies replay
and remote state, and reconnects with old packets still available. A second test runs the production
Bun daemon and Rust authority against two TCP fixtures, verifies both
authoritative and remote positions, then reconnects one fixture and checks
that its position survives. These are loopback peers, not physical devices.
Other tests cover duplicate action input, wrong-owner/epoch packets, incompatible rules,
truncation, NaNs, input floods, history exhaustion and authority time budgets.

The paired `island.stats` reply reports `online`, `playerId`, `remoteOnline`,
`remoteX/Z/Tick`, `inputAck`, `inputPredicted`, `inputPending`, `corrections`,
`replayedSteps`, `networkRejected` and `predictionStalled`, alongside measured
frame timings. `multiplayer-probe` checks movement in both directions, then
collects 20 timing windows per console during a repeating walking tape. Its
report separates movement acceptance from the 59.5 FPS / 25 ms frame-time
target and saves both devices' GPU screenshots. Daemon stderr emits a room receipt once per second.

Build, FTP readback, emulator rendering, two physical device connections,
scripted cross-device movement, hardware frame timing and human control feel
are separate evidence. A two-client simulation test is not a two-console
performance measurement.
