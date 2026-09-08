//! Island room protocol and client/authority adapters. The history algorithms
//! live in pocket-sim; movement and avatar schema remain application policy.
use crate::{Action, Input, Motion, MotionConfig};
use pocket_sim::{CommandQueue, Predictor, Timeline};
use pocket3d_anim::glam::Vec3;

pub const HISTORY: usize = 64;
pub const WIRE_BYTES: usize = 512;
const INPUT_BATCH: usize = 4;
const REMOTE_DELAY: f64 = 3.0;

/// Rule compatibility includes the actual reducer, layout and authored clips.
/// This is a compatibility fingerprint, not an authentication primitive.
pub fn rules_id() -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for bytes in [
        include_bytes!("motion.rs").as_slice(),
        include_bytes!("lib.rs").as_slice(),
        include_bytes!("net.rs").as_slice(),
        include_bytes!("../assets/layout.rs").as_slice(),
        include_bytes!("../assets/mira.p3m").as_slice(),
    ] {
        for b in bytes {
            hash = (hash ^ u64::from(*b)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Command {
    pub x: i16,
    pub z: i16,
    pub flags: u8,
    pub expression: u8,
}
impl Command {
    pub fn new(input: Input, expression: usize) -> Self {
        fn axis(x: f32) -> i16 {
            if x.is_finite() {
                libm::roundf(x.clamp(-1., 1.) * 32767.) as i16
            } else {
                0
            }
        }
        Self {
            x: axis(input.x),
            z: axis(input.z),
            flags: u8::from(input.run)
                | u8::from(input.wave) << 1
                | u8::from(input.sit) << 2
                | u8::from(input.cheer) << 3,
            expression: expression.min(6) as u8,
        }
    }
    pub fn apply(&self, state: &mut Motion, config: &MotionConfig) {
        state.expression = self.expression as usize;
        state.advance(
            Input {
                x: self.x as f32 / 32767.,
                z: self.z as f32 / 32767.,
                run: self.flags & 1 != 0,
                wave: self.flags & 2 != 0,
                sit: self.flags & 4 != 0,
                cheer: self.flags & 8 != 0,
            },
            config,
        );
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Packet {
    Hello {
        nonce: u32,
        rules: u64,
    },
    Welcome {
        nonce: u32,
        epoch: u64,
        player: u8,
        tick: u64,
        state: Motion,
    },
    Inputs {
        epoch: u64,
        first: u64,
        count: u8,
        commands: [Command; INPUT_BATCH],
    },
    Snapshot {
        epoch: u64,
        tick: u64,
        ack: u64,
        state: Motion,
        remote_epoch: u64,
        remote: Motion,
    },
}
// Hex over the existing bounded UTF-8 mailbox avoids JSON float parsing and
// allocation on the handheld. Integers and IEEE f32 fields are little-endian.
struct Writer<'a> {
    out: &'a mut [u8],
    n: usize,
}
impl Writer<'_> {
    fn byte(&mut self, b: u8) {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        self.out[self.n] = HEX[(b >> 4) as usize];
        self.out[self.n + 1] = HEX[(b & 15) as usize];
        self.n += 2;
    }
    fn bytes(&mut self, p: &[u8]) {
        for b in p {
            self.byte(*b);
        }
    }
    fn u64(&mut self, x: u64) {
        self.bytes(&x.to_le_bytes());
    }
    fn u32(&mut self, x: u32) {
        self.bytes(&x.to_le_bytes());
    }
    fn state(&mut self, s: Motion) {
        self.u64(s.tick);
        for x in [
            s.position.x,
            s.position.y,
            s.position.z,
            s.yaw,
            s.action_time,
        ] {
            self.u32(x.to_bits());
        }
        self.byte(s.action as u8);
        self.byte(s.expression as u8);
        self.byte(u8::from(s.on_bench));
    }
}
struct Reader<'a> {
    data: &'a [u8],
    n: usize,
}
impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        fn digit(x: u8) -> Option<u8> {
            match x {
                b'0'..=b'9' => Some(x - b'0'),
                b'a'..=b'f' => Some(x - b'a' + 10),
                _ => None,
            }
        }
        let b = digit(*self.data.get(self.n)?)? * 16 + digit(*self.data.get(self.n + 1)?)?;
        self.n += 2;
        Some(b)
    }
    fn bytes<const N: usize>(&mut self) -> Option<[u8; N]> {
        let mut p = [0; N];
        for b in &mut p {
            *b = self.byte()?;
        }
        Some(p)
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.bytes()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes()?))
    }
    fn state(&mut self) -> Option<Motion> {
        let tick = self.u64()?;
        let mut f = [0.; 5];
        for x in &mut f {
            *x = f32::from_bits(self.u32()?);
            if !x.is_finite() {
                return None;
            }
        }
        if f[0].abs() > 12.
            || f[1].abs() > 1.
            || f[2].abs() > 12.
            || f[3].abs() > 10000.
            || !(0.0..=1e8).contains(&f[4])
        {
            return None;
        }
        let action = match self.byte()? {
            0 => Action::Idle,
            1 => Action::Walk,
            2 => Action::Run,
            3 => Action::SitDown,
            4 => Action::SitIdle,
            5 => Action::StandUp,
            6 => Action::Wave,
            7 => Action::Cheer,
            _ => return None,
        };
        let expression = self.byte()? as usize;
        if expression > 6 {
            return None;
        }
        let bench = self.byte()?;
        if bench > 1 {
            return None;
        }
        Some(Motion {
            position: Vec3::new(f[0], f[1], f[2]),
            yaw: f[3],
            action_time: f[4],
            tick,
            action,
            expression,
            on_bench: bench != 0,
        })
    }
}
impl Packet {
    pub fn encode(&self, out: &mut [u8; WIRE_BYTES]) -> usize {
        let mut w = Writer { out, n: 0 };
        w.bytes(b"PI");
        w.byte(1);
        match self {
            Self::Hello { nonce, rules } => {
                w.byte(1);
                w.u32(*nonce);
                w.u64(*rules);
            }
            Self::Welcome {
                nonce,
                epoch,
                player,
                tick,
                state,
            } => {
                w.byte(2);
                w.u32(*nonce);
                w.u64(*epoch);
                w.byte(*player);
                w.u64(*tick);
                w.state(*state);
            }
            Self::Inputs {
                epoch,
                first,
                count,
                commands,
            } => {
                w.byte(3);
                w.u64(*epoch);
                w.u64(*first);
                w.byte(*count);
                for c in commands.iter().take(*count as usize) {
                    w.bytes(&c.x.to_le_bytes());
                    w.bytes(&c.z.to_le_bytes());
                    w.byte(c.flags);
                    w.byte(c.expression);
                }
            }
            Self::Snapshot {
                epoch,
                tick,
                ack,
                state,
                remote_epoch,
                remote,
            } => {
                w.byte(4);
                w.u64(*epoch);
                w.u64(*tick);
                w.u64(*ack);
                w.state(*state);
                w.u64(*remote_epoch);
                w.state(*remote);
            }
        }
        w.n
    }
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() > WIRE_BYTES || !data.len().is_multiple_of(2) {
            return None;
        }
        let mut r = Reader { data, n: 0 };
        if r.bytes::<3>()? != *b"PI\x01" {
            return None;
        }
        let packet = match r.byte()? {
            1 => Self::Hello {
                nonce: r.u32()?,
                rules: r.u64()?,
            },
            2 => {
                let nonce = r.u32()?;
                let epoch = r.u64()?;
                let player = r.byte()?;
                if epoch == 0 || !(1..=2).contains(&player) {
                    return None;
                }
                Self::Welcome {
                    nonce,
                    epoch,
                    player,
                    tick: r.u64()?,
                    state: r.state()?,
                }
            }
            3 => {
                let epoch = r.u64()?;
                let first = r.u64()?;
                let count = r.byte()?;
                if first == 0
                    || !(1..=INPUT_BATCH as u8).contains(&count)
                    || first.checked_add(u64::from(count) - 1).is_none()
                {
                    return None;
                }
                let mut commands = [Command::default(); INPUT_BATCH];
                for c in commands.iter_mut().take(count as usize) {
                    c.x = i16::from_le_bytes(r.bytes()?);
                    c.z = i16::from_le_bytes(r.bytes()?);
                    c.flags = r.byte()?;
                    c.expression = r.byte()?;
                    if c.flags > 15 || c.expression > 6 || c.x == i16::MIN || c.z == i16::MIN {
                        return None;
                    }
                }
                Self::Inputs {
                    epoch,
                    first,
                    count,
                    commands,
                }
            }
            4 => Self::Snapshot {
                epoch: r.u64()?,
                tick: r.u64()?,
                ack: r.u64()?,
                state: r.state()?,
                remote_epoch: r.u64()?,
                remote: r.state()?,
            },
            _ => return None,
        };
        (r.n == data.len()).then_some(packet)
    }
}

pub struct Client {
    config: MotionConfig,
    rules: u64,
    transport: i32,
    nonce: u32,
    pub epoch: u64,
    pub player: u8,
    prediction: Option<Predictor<Motion, Command, HISTORY>>,
    sent: u64,
    frame: u64,
    hello_at: Option<u64>,
    received_at: u64,
    remote_epoch: u64,
    remote: Timeline<Motion, 12>,
    server_tick: u64,
    playback: f64,
    pub corrections: u32,
    pub replayed: u32,
    pub rejected: u32,
    pub stalled: u32,
}
impl Client {
    pub fn new(config: MotionConfig) -> Self {
        Self {
            config,
            rules: rules_id(),
            transport: 0,
            nonce: 0,
            epoch: 0,
            player: 0,
            prediction: None,
            sent: 0,
            frame: 0,
            hello_at: None,
            received_at: 0,
            remote_epoch: 0,
            remote: Timeline::new(),
            server_tick: 0,
            playback: 0.,
            corrections: 0,
            replayed: 0,
            rejected: 0,
            stalled: 0,
        }
    }
    fn reset(&mut self) {
        self.nonce = self.nonce.wrapping_add(1).max(1);
        self.epoch = 0;
        self.prediction = None;
        self.sent = 0;
        self.hello_at = None;
        self.remote_epoch = 0;
        self.remote = Timeline::new();
        self.player = 0;
    }
    /// One display-frame boundary. The transport generation fences queues;
    /// the room epoch and hello nonce fence application state within a link.
    pub fn frame(&mut self, transport: i32) {
        self.frame += 1;
        if let Some(latest) = self.remote.latest_tick() {
            self.playback = (self.playback + 0.5).min(latest as f64);
        }
        if transport != self.transport || (self.epoch != 0 && self.frame - self.received_at > 180) {
            self.transport = transport;
            self.reset();
        }
    }
    pub fn receive(&mut self, p: Packet) {
        match p {
            Packet::Welcome {
                nonce,
                epoch,
                player,
                tick,
                state,
            } if self.transport > 0 && self.epoch == 0 && nonce == self.nonce => {
                self.epoch = epoch;
                self.player = player;
                self.sent = 0;
                self.prediction = Some(Predictor::new(0, state));
                self.server_tick = tick;
                self.playback = tick as f64 - REMOTE_DELAY;
                self.received_at = self.frame;
            }
            Packet::Snapshot {
                epoch,
                tick,
                ack,
                state,
                remote_epoch,
                remote,
            } if self.epoch != 0 && epoch == self.epoch && tick > self.server_tick => {
                let p = self.prediction.as_mut().unwrap();
                if ack > p.tick() {
                    self.rejected += 1;
                    return;
                }
                if ack > p.confirmed()
                    && let Ok(result) = p.reconcile(ack, state, |s, i| i.apply(s, &self.config))
                {
                    self.corrections += u32::from(result.corrected);
                    self.replayed += result.replayed as u32;
                }
                self.server_tick = tick;
                self.playback = self.playback.max(tick as f64 - REMOTE_DELAY);
                self.received_at = self.frame;
                if self.remote_epoch != remote_epoch {
                    self.remote = Timeline::new();
                    self.remote_epoch = remote_epoch;
                }
                if remote_epoch != 0 {
                    self.remote.push(tick, remote);
                }
            }
            _ => {
                self.rejected += 1;
            }
        }
    }
    pub fn step(&mut self, input: Input, expression: usize) -> Option<Motion> {
        let p = self.prediction.as_mut()?;
        if p.predict(Command::new(input, expression), |s, i| {
            i.apply(s, &self.config)
        })
        .is_err()
        {
            self.stalled += 1;
        }
        Some(*p.state())
    }
    pub fn outgoing(&self) -> Option<Packet> {
        if self.transport <= 0 {
            return None;
        }
        if self.epoch == 0 {
            return (self.hello_at.is_none_or(|f| self.frame - f >= 30)).then_some(Packet::Hello {
                nonce: self.nonce,
                rules: self.rules,
            });
        }
        let p = self.prediction.as_ref()?;
        let mut it = p.pending().filter(|t| t.tick > self.sent);
        let first = it.next()?;
        let mut commands = [Command::default(); INPUT_BATCH];
        commands[0] = first.input;
        let mut count = 1;
        for t in it.take(INPUT_BATCH - 1) {
            commands[count] = t.input;
            count += 1;
        }
        Some(Packet::Inputs {
            epoch: self.epoch,
            first: first.tick,
            count: count as u8,
            commands,
        })
    }
    pub fn sent(&mut self, packet: &Packet) {
        match packet {
            Packet::Hello { .. } => self.hello_at = Some(self.frame),
            Packet::Inputs { first, count, .. } => self.sent = first + u64::from(*count) - 1,
            _ => {}
        }
    }
    pub fn pending(&self) -> usize {
        self.prediction.as_ref().map_or(0, |p| p.pending().len())
    }
    pub fn acknowledged(&self) -> u64 {
        self.prediction.as_ref().map_or(0, Predictor::confirmed)
    }
    pub fn predicted(&self) -> u64 {
        self.prediction.as_ref().map_or(0, Predictor::tick)
    }
    pub fn remote_state(&self) -> Option<Motion> {
        if self.remote_epoch == 0 {
            return None;
        }
        let s = self.remote.sample(self.playback)?;
        let mut out = *s.from;
        out.position = s.from.position.lerp(s.to.position, s.alpha);
        let delta = libm::atan2f(
            libm::sinf(s.to.yaw - s.from.yaw),
            libm::cosf(s.to.yaw - s.from.yaw),
        );
        out.yaw += delta * s.alpha;
        if s.from.action == s.to.action && s.from.on_bench == s.to.on_bench {
            out.action_time += (s.to.action_time - s.from.action_time) * s.alpha;
        } else if s.alpha >= 1. {
            out = *s.to;
        }
        Some(out)
    }
}

pub struct Peer {
    pub state: Motion,
    pub epoch: u64,
    pub ack: u64,
    nonce: u32,
    inputs: CommandQueue<Command, HISTORY>,
    credit: u8,
    last_seen: u64,
}
pub struct Room {
    config: MotionConfig,
    rules: u64,
    next_epoch: u64,
    pub tick: u64,
    pub peers: [Peer; 2],
}
impl Room {
    pub fn new(config: MotionConfig, epoch_seed: u64) -> Self {
        Self {
            config,
            rules: rules_id(),
            next_epoch: epoch_seed.max(1),
            tick: 0,
            peers: core::array::from_fn(|i| Peer {
                state: Motion {
                    position: Vec3::new(if i == 0 { -0.7 } else { 0.7 }, 0.11, 1.6),
                    ..Motion::default()
                },
                epoch: 0,
                ack: 0,
                nonce: 0,
                inputs: CommandQueue::new(0),
                credit: 0,
                last_seen: 0,
            }),
        }
    }
    pub fn leave(&mut self, peer: usize) {
        if let Some(p) = self.peers.get_mut(peer) {
            p.epoch = 0;
            p.inputs = CommandQueue::new(0);
        }
    }
    pub fn receive(&mut self, peer: usize, packet: Packet) -> Result<Option<Packet>, &'static str> {
        let p = self.peers.get_mut(peer).ok_or("unknown player")?;
        match packet {
            Packet::Hello { nonce, rules } => {
                if rules != self.rules {
                    return Err("incompatible simulation rules");
                }
                if p.epoch == 0 || p.nonce != nonce {
                    self.next_epoch = self.next_epoch.checked_add(1).ok_or("session exhausted")?;
                    p.epoch = self.next_epoch;
                    p.nonce = nonce;
                    p.ack = 0;
                    p.inputs = CommandQueue::new(0);
                    p.credit = 0;
                }
                p.last_seen = self.tick;
                Ok(Some(Packet::Welcome {
                    nonce,
                    epoch: p.epoch,
                    player: peer as u8 + 1,
                    tick: self.tick,
                    state: p.state,
                }))
            }
            Packet::Inputs {
                epoch,
                first,
                count,
                commands,
            } if p.epoch != 0 && epoch == p.epoch => {
                if count == 0 || count as usize > INPUT_BATCH {
                    return Err("invalid batch");
                }
                for (i, c) in commands.iter().take(count as usize).enumerate() {
                    p.inputs
                        .push(first.checked_add(i as u64).ok_or("sequence exhausted")?, *c)
                        .map_err(|_| "input gap or backlog")?;
                }
                p.last_seen = self.tick;
                Ok(None)
            }
            _ => Err("wrong session or packet direction"),
        }
    }
    /// One 30 Hz authority tick grants one step of time to each player, with
    /// at most three banked steps for packet jitter. Packet count grants none.
    pub fn advance(&mut self) {
        self.tick += 1;
        for p in &mut self.peers {
            if p.epoch == 0 {
                continue;
            }
            if self.tick - p.last_seen > 90 {
                p.epoch = 0;
                p.inputs = CommandQueue::new(0);
                continue;
            }
            p.credit = (p.credit + 1).min(3);
            while p.credit > 0 {
                let Some((tick, command)) = p.inputs.pop() else {
                    break;
                };
                command.apply(&mut p.state, &self.config);
                p.ack = tick;
                p.credit -= 1;
            }
        }
    }
    pub fn snapshot(&self, peer: usize) -> Option<Packet> {
        let p = self.peers.get(peer)?;
        if p.epoch == 0 {
            return None;
        }
        let remote = &self.peers[1 - peer];
        Some(Packet::Snapshot {
            epoch: p.epoch,
            tick: self.tick,
            ack: p.ack,
            state: p.state,
            remote_epoch: remote.epoch,
            remote: remote.state,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::VecDeque;
    fn config() -> MotionConfig {
        crate::Island::new().config
    }
    fn wire(p: &Packet) -> Packet {
        let mut out = [0; WIRE_BYTES];
        let n = p.encode(&mut out);
        Packet::decode(&out[..n]).expect("valid packet roundtrip")
    }
    fn join(room: &mut Room, client: &mut Client, peer: usize, generation: i32) {
        client.frame(generation);
        let hello = client.outgoing().unwrap();
        client.sent(&hello);
        client.receive(wire(&room.receive(peer, wire(&hello)).unwrap().unwrap()));
        assert_ne!(client.epoch, 0);
    }
    #[test]
    fn two_clients_predict_correct_interpolate_and_rejoin_over_impaired_wire() {
        let cfg = config();
        let mut room = Room::new(cfg.clone(), 100);
        let mut clients = [Client::new(cfg.clone()), Client::new(cfg)];
        for (i, c) in clients.iter_mut().enumerate() {
            join(&mut room, c, i, 1);
        }
        let mut up: [VecDeque<(u64, Packet)>; 2] = core::array::from_fn(|_| VecDeque::new());
        let mut down: [VecDeque<(u64, Packet)>; 2] = core::array::from_fn(|_| VecDeque::new());
        let mut queued_at = [0; 2];
        for frame in 0..800 {
            for i in 0..2 {
                clients[i].frame(1);
                while down[i].front().is_some_and(|p| p.0 <= frame) {
                    clients[i].receive(down[i].pop_front().unwrap().1);
                }
                while up[i].front().is_some_and(|p| p.0 <= frame) {
                    room.receive(i, up[i].pop_front().unwrap().1).unwrap();
                }
                if frame % 2 == 0 {
                    let input = Input {
                        x: if i == 0 && frame < 280 { 0.45 } else { 0. },
                        z: if i == 1 && frame < 280 { -0.3 } else { 0. },
                        wave: frame == 300,
                        ..Input::default()
                    };
                    clients[i].step(input, i);
                }
                // TCP loss becomes delay; application snapshots can be skipped.
                if frame % 4 == 0
                    && let Some(p) = clients[i].outgoing()
                {
                    clients[i].sent(&p);
                    queued_at[i] = queued_at[i].max(frame + (frame * 7 + i as u64 * 3) % 9);
                    up[i].push_back((queued_at[i], wire(&p)));
                }
            }
            if frame == 180 {
                room.peers[0].state.position.x -= 0.12;
            }
            if frame % 2 == 0 {
                room.advance();
            }
            if frame % 4 == 0 && frame % 28 != 0 {
                for (i, queue) in down.iter_mut().enumerate() {
                    let due = queue
                        .back()
                        .map_or(0, |p| p.0)
                        .max(frame + (frame + i as u64) % 7);
                    queue.push_back((due, wire(&room.snapshot(i).unwrap())));
                }
            }
        }
        for (i, c) in clients.iter().enumerate() {
            assert!(c.pending() <= 16);
            assert!(
                c.prediction
                    .as_ref()
                    .unwrap()
                    .state()
                    .position
                    .distance(room.peers[i].state.position)
                    < 1e-5
            );
            assert!(
                c.remote_state()
                    .unwrap()
                    .position
                    .distance(room.peers[1 - i].state.position)
                    < 1e-5
            );
            assert_eq!(c.remote_state().unwrap().expression, 1 - i);
        }
        assert!(clients[0].corrections > 0 && clients[0].replayed > 0);
        let old = room.snapshot(0).unwrap();
        let position = room.peers[0].state.position;
        room.leave(0);
        clients[0].frame(0);
        join(&mut room, &mut clients[0], 0, 2);
        clients[0].receive(old);
        assert_eq!(
            clients[0].prediction.as_ref().unwrap().state().position,
            position
        );
        assert_eq!(clients[0].pending(), 0);
        assert!(clients[0].rejected > 0);
    }
    #[test]
    fn budgets_bound_stalls_server_time_and_packet_validation() {
        let cfg = config();
        let mut room = Room::new(cfg.clone(), 200);
        let mut c = Client::new(cfg);
        join(&mut room, &mut c, 0, 1);
        for _ in 0..100 {
            c.step(
                Input {
                    x: 1.,
                    ..Input::default()
                },
                0,
            );
        }
        assert_eq!(c.pending(), HISTORY);
        assert_eq!(c.predicted(), HISTORY as u64);
        assert_eq!(c.stalled, 36);
        while let Some(packet) = c.outgoing() {
            room.receive(0, wire(&packet)).unwrap();
            c.sent(&packet);
        }
        room.advance();
        assert_eq!(room.peers[0].ack, 1);
        for _ in 0..9 {
            room.advance();
        }
        assert_eq!(room.peers[0].ack, 10);
        let mut bytes = [0; WIRE_BYTES];
        let packet = room.snapshot(0).unwrap();
        let n = packet.encode(&mut bytes);
        for end in 0..n {
            assert!(Packet::decode(&bytes[..end]).is_none());
        }
        assert_eq!(Packet::decode(&bytes[..n]), Some(packet));
        bytes[n] = b'0';
        bytes[n + 1] = b'0';
        assert!(Packet::decode(&bytes[..n + 2]).is_none());
        let mut invalid = bytes[..n].to_vec();
        invalid[0] = b'g';
        assert!(Packet::decode(&invalid).is_none());
        assert!(
            room.receive(1, Packet::Hello { nonce: 1, rules: 0 })
                .is_err()
        );
    }
    #[test]
    fn disconnect_and_old_epoch_cannot_move_another_player_or_replay_edges() {
        let cfg = config();
        let mut room = Room::new(cfg.clone(), 9);
        let mut c = Client::new(cfg);
        join(&mut room, &mut c, 0, 1);
        c.step(
            Input {
                wave: true,
                ..Input::default()
            },
            2,
        );
        let command = c.outgoing().unwrap();
        assert!(room.receive(1, command.clone()).is_err());
        room.receive(0, command.clone()).unwrap();
        room.receive(0, command.clone()).unwrap();
        room.advance();
        assert_eq!(room.peers[0].ack, 1);
        assert_eq!(room.peers[0].state.action, Action::Wave);
        let time = room.peers[0].state.action_time;
        room.receive(0, command.clone()).unwrap();
        room.advance();
        assert_eq!(room.peers[0].state.action_time, time);
        room.leave(0);
        c.frame(0);
        join(&mut room, &mut c, 0, 2);
        assert!(room.receive(0, command).is_err());
        for _ in 0..91 {
            room.advance();
        }
        assert!(room.snapshot(0).is_none());
    }
    #[test]
    fn wire_rejects_nonfinite_state_and_invalid_input_fields() {
        let cfg = config();
        let mut room = Room::new(cfg.clone(), 1);
        let mut c = Client::new(cfg);
        join(&mut room, &mut c, 0, 1);
        room.peers[0].state.position.x = f32::NAN;
        let mut bytes = [0; WIRE_BYTES];
        let n = room.snapshot(0).unwrap().encode(&mut bytes);
        assert!(Packet::decode(&bytes[..n]).is_none());
        for (flags, expression, x) in [(16, 0, 0), (0, 7, 0), (0, 0, i16::MIN)] {
            let p = Packet::Inputs {
                epoch: 1,
                first: 1,
                count: 1,
                commands: [Command {
                    flags,
                    expression,
                    x,
                    ..Command::default()
                }; INPUT_BATCH],
            };
            let n = p.encode(&mut bytes);
            assert!(Packet::decode(&bytes[..n]).is_none());
        }
    }
}
