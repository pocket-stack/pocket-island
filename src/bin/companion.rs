//! Local authority process. Desktop socket ownership stays in the shared
//! companion-session transport; stdin supplies bounded ordered room commands.
use pocket_island::{
    Island,
    net::{Packet, Room, WIRE_BYTES},
};
use std::io::{self, BufRead, Read, Write};
fn emit(out: &mut impl Write, peer: usize, packet: Packet) -> io::Result<()> {
    let mut bytes = [0; WIRE_BYTES];
    let n = packet.encode(&mut bytes);
    writeln!(
        out,
        "P {peer} {}",
        std::str::from_utf8(&bytes[..n]).unwrap()
    )
}
fn main() -> io::Result<()> {
    let seed = std::env::args()
        .nth(1)
        .and_then(|x| x.parse().ok())
        .unwrap_or(1);
    let mut room = Room::new(Island::new().config, seed);
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut line = String::with_capacity(1024);
    loop {
        line.clear();
        let n = input.by_ref().take(1025).read_line(&mut line)?;
        if n == 0 {
            break;
        }
        if n > 1024 || !line.ends_with('\n') {
            return Err(io::Error::other("authority input budget"));
        }
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("T") => {
                room.advance();
                if room.tick.is_multiple_of(2) {
                    for i in 0..2 {
                        if let Some(packet) = room.snapshot(i) {
                            emit(&mut out, i, packet)?;
                        }
                    }
                }
                // Receipt allows the daemon to bound its process mailbox.
                writeln!(out, "T {}", room.tick)?;
            }
            Some("P") => {
                let peer: usize = parts
                    .next()
                    .and_then(|s| s.parse().ok())
                    .filter(|i| *i < 2)
                    .ok_or_else(|| io::Error::other("player index"))?;
                let packet = parts.next().and_then(|s| Packet::decode(s.as_bytes()));
                match packet.map(|p| room.receive(peer, p)) {
                    Some(Ok(Some(reply))) => emit(&mut out, peer, reply)?,
                    Some(Ok(None)) => {}
                    _ => {
                        room.leave(peer);
                        writeln!(out, "D {peer}")?;
                    }
                }
            }
            Some("L") => {
                if let Some(peer) = parts.next().and_then(|s| s.parse().ok()) {
                    room.leave(peer);
                }
            }
            Some("S") => {
                let p = &room.peers;
                eprintln!(
                    "{{\"tick\":{},\"players\":[{{\"id\":1,\"online\":{},\"ack\":{},\"x\":{},\"z\":{},\"action\":{}}},{{\"id\":2,\"online\":{},\"ack\":{},\"x\":{},\"z\":{},\"action\":{}}}]}}",
                    room.tick,
                    p[0].epoch != 0,
                    p[0].ack,
                    p[0].state.position.x,
                    p[0].state.position.z,
                    p[0].state.action as u32,
                    p[1].epoch != 0,
                    p[1].ack,
                    p[1].state.position.x,
                    p[1].state.position.z,
                    p[1].state.action as u32
                );
            }
            _ => return Err(io::Error::other("authority command")),
        }
        out.flush()?;
    }
    Ok(())
}
