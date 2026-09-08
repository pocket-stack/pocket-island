#![no_std]
extern crate alloc;
use alloc::boxed::Box;
use core::{
    alloc::{GlobalAlloc, Layout},
    ffi::c_void,
};
use pocket_island::{Input, Island};
use pocket3d_mesh::rigid::{RigidMesh, RigidRange, RigidVertex, SkinMatrix};
unsafe extern "C" {
    fn memalign(align: usize, size: usize) -> *mut c_void;
    fn free(p: *mut c_void);
    fn abort() -> !;
}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        unsafe { memalign(l.align().max(8), l.size().max(1)).cast() }
    }
    unsafe fn dealloc(&self, p: *mut u8, _: Layout) {
        unsafe { free(p.cast()) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { abort() }
}
#[repr(C)]
pub struct SkinLight {
    direction: [f32; 3],
    ambient: f32,
    diffuse: f32,
}
const _: () = assert!(core::mem::size_of::<SkinLight>() == 20);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_light(out: *mut SkinLight) {
    let (direction, ambient, diffuse) = pocket_island::light_parameters();
    unsafe {
        *out = SkinLight {
            direction,
            ambient,
            diffuse,
        };
    }
}

#[repr(C)]
pub struct Snapshot {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub cam_x: f32,
    pub cam_z: f32,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub anchor_z: f32,
    pub action: u32,
    pub expression: u32,
    pub tick: u32,
    pub messages: u32,
    pub action_time: f32,
}
#[unsafe(no_mangle)]
pub extern "C" fn island_new() -> *mut Island {
    Box::into_raw(Box::new(Island::new()))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_replica(
    s: *const Island,
    x: f32,
    z: f32,
    phase: f32,
) -> *mut Island {
    Box::into_raw(Box::new(unsafe { &*s }.replica(x, z, phase)))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_skin_new(s: *const Island) -> *mut RigidMesh {
    match unsafe { &*s }.actor.rigid_mesh() {
        Ok(mesh) => Box::into_raw(Box::new(mesh)),
        Err(_) => core::ptr::null_mut(),
    }
}
#[repr(C)]
pub struct SkinSource {
    vertices: *const RigidVertex,
    indices: *const u16,
    ranges: *const RigidRange,
    vertex_count: u32,
    index_count: u32,
    range_count: u32,
    joint_count: u32,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_skin_source(m: *const RigidMesh, out: *mut SkinSource) {
    let m = unsafe { &*m };
    unsafe {
        *out = SkinSource {
            vertices: m.vertices.as_ptr(),
            indices: m.indices.as_ptr(),
            ranges: m.ranges.as_ptr(),
            vertex_count: m.vertices.len() as u32,
            index_count: m.indices.len() as u32,
            range_count: m.ranges.len() as u32,
            joint_count: m.joints as u32,
        };
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_skin_free(m: *mut RigidMesh) {
    if !m.is_null() {
        drop(unsafe { Box::from_raw(m) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_palette(s: *const Island, count: *mut u32) -> *const SkinMatrix {
    let p = &unsafe { &*s }.skin_palette;
    unsafe {
        *count = p.len() as u32;
    }
    p.as_ptr()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_shadow(s: *const Island, out: *mut f32) {
    let p = unsafe { &*s }.shadow_position().to_array();
    unsafe {
        core::ptr::copy_nonoverlapping(p.as_ptr(), out, 3);
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_free(s: *mut Island) {
    if !s.is_null() {
        drop(unsafe { Box::from_raw(s) })
    }
}
fn host_input(x: f32, z: f32, flags: u32) -> Input {
    Input {
        x,
        z,
        run: flags & 1 != 0,
        wave: flags & 2 != 0,
        sit: flags & 4 != 0,
        cheer: flags & 8 != 0,
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_step(s: *mut Island, x: f32, z: f32, flags: u32) {
    unsafe { &mut *s }.advance(host_input(x, z, flags));
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_expression(s: *mut Island, e: u32) {
    unsafe { (*s).set_expression(e as usize) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_vertices(
    s: *const Island,
    terrain: bool,
    count: *mut u32,
) -> *const c_void {
    let v = if terrain {
        &unsafe { &*s }.terrain
    } else {
        &unsafe { &*s }.character
    };
    unsafe {
        *count = v.len() as u32;
    }
    v.as_ptr().cast()
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_snapshot(s: *const Island, out: *mut Snapshot) {
    let s = unsafe { &*s };
    let a = s.bubble_anchor();
    unsafe {
        *out = Snapshot {
            x: s.motion.position.x,
            y: s.motion.position.y,
            z: s.motion.position.z,
            cam_x: s.render_camera.x,
            cam_z: s.render_camera.z,
            anchor_x: a.x,
            anchor_y: a.y,
            anchor_z: a.z,
            action: s.motion.action as u32,
            expression: s.motion.expression as u32,
            tick: s.motion.tick as u32,
            messages: s.chat.history.len() as u32,
            action_time: s.motion.action_time,
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_send(s: *mut Island, text: *const u8, len: u32) -> i32 {
    if text.is_null() || len > 192 {
        return -1;
    }
    let Ok(text) = core::str::from_utf8(unsafe { core::slice::from_raw_parts(text, len as usize) })
    else {
        return -1;
    };
    match unsafe { &mut *s }.send(text) {
        Ok(_) => 0,
        Err(pocket_island::MessageError::RateLimited) => -2,
        Err(_) => -1,
    }
}
unsafe fn copy_text(text: &str, out: *mut u8, capacity: u32) -> u32 {
    if capacity == 0 || out.is_null() {
        return 0;
    }
    let mut n = text.len().min(capacity as usize - 1);
    while !text.is_char_boundary(n) {
        n -= 1;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(text.as_ptr(), out, n);
        *out.add(n) = 0;
    }
    n as u32
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_bubble(s: *const Island, out: *mut u8, capacity: u32) -> u32 {
    let s = unsafe { &*s };
    unsafe {
        copy_text(
            s.chat
                .bubble(s.chat.local_peer, s.ui_tick)
                .map(|m| m.body.as_str())
                .unwrap_or(""),
            out,
            capacity,
        )
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_message(
    s: *const Island,
    index: u32,
    out: *mut u8,
    capacity: u32,
) -> u32 {
    let s = unsafe { &*s };
    unsafe {
        copy_text(
            s.chat
                .history
                .iter()
                .rev()
                .nth(index as usize)
                .map(|m| m.body.as_str())
                .unwrap_or(""),
            out,
            capacity,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_present(s: *mut Island, alpha: f32) {
    unsafe { (*s).present_pose(alpha) }
}

use pocket_island::net::{Client, Packet, WIRE_BYTES};
pub struct NativeNet {
    client: Client,
    remote: Box<Island>,
    pending: Option<Packet>,
}
#[repr(C)]
pub struct NetworkSnapshot {
    linked: u32,
    player: u32,
    remote: u32,
    pending: u32,
    acknowledged: u32,
    predicted: u32,
    corrections: u32,
    replayed: u32,
    rejected: u32,
    stalled: u32,
    remote_x: f32,
    remote_z: f32,
    remote_tick: u32,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_net_new(s: *const Island) -> *mut NativeNet {
    let s = unsafe { &*s };
    Box::into_raw(Box::new(NativeNet {
        client: Client::new(s.config.clone()),
        remote: Box::new(s.replica(0.7, 1.6, 0.)),
        pending: None,
    }))
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_net_free(n: *mut NativeNet) {
    if !n.is_null() {
        drop(unsafe { Box::from_raw(n) });
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_net_frame(n: *mut NativeNet, session: i32) {
    unsafe { &mut *n }.client.frame(session);
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_net_receive(n: *mut NativeNet, data: *const u8, length: u32) {
    let n = unsafe { &mut *n };
    if length as usize > WIRE_BYTES {
        n.client.rejected += 1;
        return;
    }
    let bytes = unsafe { core::slice::from_raw_parts(data, length as usize) };
    if let Some(packet) = Packet::decode(bytes) {
        n.client.receive(packet);
    } else {
        n.client.rejected += 1;
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_net_outgoing(n: *mut NativeNet, out: *mut u8) -> u32 {
    let n = unsafe { &mut *n };
    n.pending = n.client.outgoing();
    let Some(packet) = &n.pending else {
        return 0;
    };
    let mut bytes = [0; WIRE_BYTES];
    let length = packet.encode(&mut bytes);
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), out, length);
    }
    length as u32
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_net_sent(n: *mut NativeNet) {
    let n = unsafe { &mut *n };
    if let Some(packet) = n.pending.take() {
        n.client.sent(&packet);
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_net_step(
    n: *mut NativeNet,
    s: *mut Island,
    x: f32,
    z: f32,
    flags: u32,
) {
    let n = unsafe { &mut *n };
    let s = unsafe { &mut *s };
    let input = host_input(x, z, flags);
    if let Some(state) = n.client.step(input, s.motion.expression) {
        s.accept_motion(state);
    } else {
        s.advance(input);
    }
    if let Some(state) = n.client.remote_state() {
        n.remote.accept_motion(state);
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_net_actor(n: *mut NativeNet) -> *mut Island {
    let n = unsafe { &mut *n };
    if n.client.remote_state().is_some() {
        &mut *n.remote
    } else {
        core::ptr::null_mut()
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn island_net_snapshot(n: *const NativeNet, out: *mut NetworkSnapshot) {
    let n = unsafe { &*n };
    let c = &n.client;
    let remote = c.remote_state();
    unsafe {
        *out = NetworkSnapshot {
            linked: u32::from(c.epoch != 0),
            player: c.player as u32,
            remote: u32::from(remote.is_some()),
            pending: c.pending() as u32,
            acknowledged: c.acknowledged() as u32,
            predicted: c.predicted() as u32,
            corrections: c.corrections,
            replayed: c.replayed,
            rejected: c.rejected,
            stalled: c.stalled,
            remote_x: remote.map_or(0., |s| s.position.x),
            remote_z: remote.map_or(0., |s| s.position.z),
            remote_tick: remote.map_or(0, |s| s.tick as u32),
        };
    }
}
