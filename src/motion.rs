//! Pure fixed-step Island movement used by clients and the companion.
use super::{Action, Input, RUN_SPEED, STEP, WALK_SPEED, layout};
use pocket3d_anim::glam::Vec3;
use pocket3d_mesh::colored::MeshAsset;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub position: Vec3,
    pub yaw: f32,
    pub action: Action,
    pub expression: usize,
    pub action_time: f32,
    pub tick: u64,
    pub on_bench: bool,
}
impl Default for Motion {
    fn default() -> Self {
        Self {
            position: Vec3::new(0., 0.11, 1.6),
            yaw: 0.,
            action: Action::Idle,
            expression: 0,
            action_time: 0.,
            tick: 0,
            on_bench: false,
        }
    }
}
/// Authored clip timings are immutable simulation configuration shared with
/// the authority. No mesh, skeleton, chat or camera state enters a checkpoint.
#[derive(Clone)]
pub struct MotionConfig {
    durations: [f32; 11],
}
impl MotionConfig {
    pub fn from_asset(asset: &MeshAsset) -> Self {
        let names = [
            "Idle",
            "Walk",
            "Run",
            "SitDown",
            "SitIdle",
            "StandUp",
            "Wave",
            "Cheer",
            "BenchSitDown",
            "BenchSitIdle",
            "BenchStandUp",
        ];
        Self {
            durations: names.map(|n| asset.clips[asset.clip(n).expect("motion clip")].duration),
        }
    }
    fn duration(&self, m: &Motion) -> f32 {
        let index = if m.on_bench
            && matches!(
                m.action,
                Action::SitDown | Action::SitIdle | Action::StandUp
            ) {
            8 + m.action as usize - Action::SitDown as usize
        } else {
            m.action as usize
        };
        self.durations[index]
    }
}
impl Motion {
    pub fn clip_name(&self) -> &'static str {
        if self.on_bench {
            match self.action {
                Action::SitDown => "BenchSitDown",
                Action::SitIdle => "BenchSitIdle",
                Action::StandUp => "BenchStandUp",
                _ => self.action.name(),
            }
        } else {
            self.action.name()
        }
    }
    pub(crate) fn change(&mut self, a: Action, config: &MotionConfig) {
        if self.action == a {
            return;
        }
        let phase = if matches!(self.action, Action::Walk | Action::Run)
            && matches!(a, Action::Walk | Action::Run)
        {
            self.action_time / config.duration(self)
        } else {
            0.
        };
        self.action = a;
        self.action_time = phase * config.duration(self);
    }
    pub fn walkable(x: f32, z: f32) -> bool {
        if !x.is_finite() || !z.is_finite() {
            return false;
        }
        let on_island = x * x / (10.1 * 10.1) + z * z / (8.1 * 8.1) < 1.;
        let on_dock = (-0.52..=0.72).contains(&x) && (6.4..=9.1).contains(&z);
        (on_island || on_dock)
            && !layout::COLLIDERS.iter().any(|&(cx, cz, r)| {
                let d = (x - cx) * (x - cx) + (z - cz) * (z - cz);
                d < (r + 0.23) * (r + 0.23)
            })
    }
    pub fn ground_height(x: f32, z: f32) -> f32 {
        if (-0.70..=0.90).contains(&x) && (6.69..=9.60).contains(&z) {
            0.17
        } else if x * x / (9.15 * 9.15) + z * z / (7.15 * 7.15) < 1.0 {
            0.11
        } else {
            -0.01
        }
    }
    pub fn advance(&mut self, input: Input, config: &MotionConfig) {
        self.tick += 1;
        if !matches!(self.action, Action::Walk | Action::Run) {
            self.action_time += STEP;
        }
        let mut dir = Vec3::new(
            if input.x.is_finite() {
                input.x.clamp(-1., 1.)
            } else {
                0.
            },
            0.,
            if input.z.is_finite() {
                input.z.clamp(-1., 1.)
            } else {
                0.
            },
        );
        if dir.length() < 0.16 {
            dir = Vec3::ZERO
        } else {
            dir = dir.clamp_length_max(1.)
        }
        let moving = dir.length_squared() > 0.;
        if input.sit && !matches!(self.action, Action::SitDown | Action::StandUp) {
            if self.action == Action::SitIdle {
                self.change(Action::StandUp, config)
            } else {
                let (x, z, _) = layout::BENCH;
                if (self.position - Vec3::new(x, 0.09, z)).length() < 1.65 {
                    self.position.x = x;
                    self.position.z = z - 0.02;
                    self.on_bench = true;
                    self.yaw = 0.;
                }
                self.yaw = 0.0;
                self.change(Action::SitDown, config)
            }
        }
        let duration = config.duration(self);
        if self.action_time >= duration {
            match self.action {
                Action::SitDown => self.change(Action::SitIdle, config),
                Action::StandUp => {
                    self.on_bench = false;
                    self.position.z += if self.near_bench() { 1.35 } else { 0. };
                    self.change(Action::Idle, config)
                }
                Action::Wave | Action::Cheer => self.change(Action::Idle, config),
                _ => {}
            }
        }
        if moving && self.action == Action::SitIdle {
            self.change(Action::StandUp, config)
        }
        if !matches!(
            self.action,
            Action::SitDown | Action::SitIdle | Action::StandUp
        ) {
            if moving {
                let step = dir * (if input.run { RUN_SPEED } else { WALK_SPEED }) * STEP;
                let previous = self.position;
                let next = self.position + step;
                if Self::walkable(next.x, self.position.z) {
                    self.position.x = next.x;
                }
                if Self::walkable(self.position.x, next.z) {
                    self.position.z = next.z;
                }
                let travelled = self.position - previous;
                let distance = travelled.length();
                if distance > 1e-6 {
                    self.change(if input.run { Action::Run } else { Action::Walk }, config);
                    let stride = if input.run {
                        layout::RUN_STRIDE
                    } else {
                        layout::WALK_STRIDE
                    };
                    let duration = config.duration(self);
                    self.action_time += distance / stride * duration;
                    let target = libm::atan2f(travelled.x, travelled.z);
                    let delta =
                        libm::atan2f(libm::sinf(target - self.yaw), libm::cosf(target - self.yaw));
                    self.yaw += delta * 0.24;
                } else if matches!(self.action, Action::Walk | Action::Run) {
                    self.change(Action::Idle, config);
                }
            } else if matches!(self.action, Action::Walk | Action::Run) {
                self.change(Action::Idle, config)
            }
            if input.wave {
                self.yaw = 0.0;
                self.change(Action::Wave, config)
            }
            if input.cheer {
                self.yaw = 0.0;
                self.change(Action::Cheer, config)
            }
        }
        self.position.y = Self::ground_height(self.position.x, self.position.z);
    }
    fn near_bench(&self) -> bool {
        let (x, z, _) = layout::BENCH;
        (self.position.x - x).abs() < 0.1 && (self.position.z - z).abs() < 0.15
    }
}
