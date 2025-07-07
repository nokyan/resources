use std::fmt::Display;

use nutype::nutype;
use serde::{Deserialize, Serialize};

use crate::pci_slot::PciSlot;

#[nutype(
    validate(less_or_equal = 100),
    validate(greater_or_equal = 0),
    derive(
        Debug,
        Default,
        Clone,
        Hash,
        PartialEq,
        Eq,
        Serialize,
        Deserialize,
        Copy,
        FromStr,
        Deref,
        TryFrom,
        Display,
        PartialOrd,
        Ord,
    ),
    default = 0
)]
pub struct Percentage(u8);

impl Percentage {
    fn fraction(self) -> f32 {
        self.into_inner() as f32 / 100.0
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize, Copy, PartialOrd, Ord)]
pub enum GpuIdentifier {
    PciSlot(PciSlot),
    Enumerator(usize),
}

impl Default for GpuIdentifier {
    fn default() -> Self {
        GpuIdentifier::Enumerator(0)
    }
}

impl Display for GpuIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuIdentifier::PciSlot(pci_slot) => write!(f, "{pci_slot}"),
            GpuIdentifier::Enumerator(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum GpuUsageStats {
    AmdgpuStats {
        gfx_ns: u64,
        enc_ns: u64,
        dec_ns: u64,
        mem_bytes: u64,
    },
    I915Stats {
        gfx_ns: u64,
        video_ns: u64,
    },
    NvidiaStats {
        gfx_percentage: Percentage,
        enc_percentage: Percentage,
        dec_percentage: Percentage,
        mem_bytes: u64,
    },
    V3dStats {
        gfx_ns: u64,
        mem_bytes: u64,
    },
    XeStats {
        gfx_cycles: u64,
        gfx_total_cycles: u64,
        video_cycles: u64,
        video_cycles_total: u64,
        mem_bytes: u64,
    },
}

impl GpuUsageStats {
    pub fn gfx_fraction(&self, old: &Self, time_delta: u64) -> Option<f32> {
        match (self, old) {
            (
                Self::AmdgpuStats {
                    gfx_ns: a_gfx_ns, ..
                },
                Self::AmdgpuStats {
                    gfx_ns: b_gfx_ns, ..
                },
            )
            | (
                Self::I915Stats {
                    gfx_ns: a_gfx_ns, ..
                },
                Self::I915Stats {
                    gfx_ns: b_gfx_ns, ..
                },
            )
            | (
                Self::V3dStats {
                    gfx_ns: a_gfx_ns, ..
                },
                Self::V3dStats {
                    gfx_ns: b_gfx_ns, ..
                },
            ) => {
                Some((a_gfx_ns.saturating_sub(*b_gfx_ns)) as f32 / (time_delta * 1_000_000) as f32)
            }
            (Self::NvidiaStats { gfx_percentage, .. }, Self::NvidiaStats { .. }) => {
                Some(gfx_percentage.fraction())
            }
            (
                Self::XeStats {
                    gfx_cycles: a_gfx_cycles,
                    gfx_total_cycles: a_gfx_total_cycles,
                    ..
                },
                Self::XeStats {
                    gfx_cycles: b_gfx_cycles,
                    gfx_total_cycles: b_gfx_total_cycles,
                    ..
                },
            ) => {
                let gfx_cycles_delta = a_gfx_cycles.saturating_sub(*b_gfx_cycles) as f64;
                let gfx_total_cycles_delta =
                    a_gfx_total_cycles.saturating_sub(*b_gfx_total_cycles) as f64;
                Some((gfx_cycles_delta / gfx_total_cycles_delta) as f32)
            }
            _ => None,
        }
    }

    pub fn enc_fraction(&self, old: &Self, time_delta: u64) -> Option<f32> {
        match (self, old) {
            (
                Self::AmdgpuStats {
                    enc_ns: a_enc_ns, ..
                },
                Self::AmdgpuStats {
                    enc_ns: b_enc_ns, ..
                },
            )
            | (
                Self::I915Stats {
                    video_ns: a_enc_ns, ..
                },
                Self::I915Stats {
                    video_ns: b_enc_ns, ..
                },
            ) => {
                Some((a_enc_ns.saturating_sub(*b_enc_ns)) as f32 / (time_delta * 1_000_000) as f32)
            }
            (Self::NvidiaStats { enc_percentage, .. }, Self::NvidiaStats { .. }) => {
                Some(enc_percentage.fraction())
            }
            (
                Self::XeStats {
                    video_cycles: a_enc_cycles,
                    video_cycles_total: a_enc_total_cycles,
                    ..
                },
                Self::XeStats {
                    video_cycles: b_enc_cycles,
                    video_cycles_total: b_enc_total_cycles,
                    ..
                },
            ) => {
                let enc_cycles_delta = a_enc_cycles.saturating_sub(*b_enc_cycles) as f64;
                let enc_total_cycles_delta =
                    a_enc_total_cycles.saturating_sub(*b_enc_total_cycles) as f64;
                Some((enc_cycles_delta / enc_total_cycles_delta) as f32)
            }
            _ => None,
        }
    }

    /// For cards with a unified media engine (i.e. no seperated encode/decode stats), this will either be 0 (in case of
    /// some AMD GPUs) or the same as enc_fraction()
    pub fn dec_fraction(&self, old: &Self, time_delta: u64) -> Option<f32> {
        match (self, old) {
            (
                Self::AmdgpuStats {
                    dec_ns: a_dec_ns, ..
                },
                Self::AmdgpuStats {
                    dec_ns: b_dec_ns, ..
                },
            ) => {
                Some((a_dec_ns.saturating_sub(*b_dec_ns)) as f32 / (time_delta * 1_000_000) as f32)
            }
            (Self::NvidiaStats { dec_percentage, .. }, Self::NvidiaStats { .. }) => {
                Some(dec_percentage.fraction())
            }
            (
                Self::XeStats {
                    video_cycles: a_enc_cycles,
                    video_cycles_total: a_enc_total_cycles,
                    ..
                },
                Self::XeStats {
                    video_cycles: b_enc_cycles,
                    video_cycles_total: b_enc_total_cycles,
                    ..
                },
            ) => {
                let enc_cycles_delta = a_enc_cycles.saturating_sub(*b_enc_cycles) as f64;
                let enc_total_cycles_delta =
                    a_enc_total_cycles.saturating_sub(*b_enc_total_cycles) as f64;
                Some((enc_cycles_delta / enc_total_cycles_delta) as f32)
            }
            _ => None,
        }
    }

    pub fn mem(&self) -> Option<u64> {
        match self {
            Self::AmdgpuStats { mem_bytes, .. }
            | Self::NvidiaStats { mem_bytes, .. }
            | Self::V3dStats { mem_bytes, .. }
            | Self::XeStats { mem_bytes, .. } => Some(*mem_bytes),
            Self::I915Stats { .. } => None,
        }
    }

    /// Returns a new stats object with the highest statistics of both objects
    pub fn greater(&self, other: &Self) -> Self {
        match (self, other) {
            (
                Self::AmdgpuStats {
                    gfx_ns: a_gfx_ns,
                    enc_ns: a_enc_ns,
                    dec_ns: a_dec_ns,
                    mem_bytes: a_mem_bytes,
                },
                Self::AmdgpuStats {
                    gfx_ns: b_gfx_ns,
                    enc_ns: b_enc_ns,
                    dec_ns: b_dec_ns,
                    mem_bytes: b_mem_bytes,
                },
            ) => Self::AmdgpuStats {
                gfx_ns: std::cmp::max(*a_gfx_ns, *b_gfx_ns),
                enc_ns: std::cmp::max(*a_enc_ns, *b_enc_ns),
                dec_ns: std::cmp::max(*a_dec_ns, *b_dec_ns),
                mem_bytes: std::cmp::max(*a_mem_bytes, *b_mem_bytes),
            },
            (
                Self::I915Stats {
                    gfx_ns: a_gfx_ns,
                    video_ns: a_video_ns,
                },
                Self::I915Stats {
                    gfx_ns: b_gfx_ns,
                    video_ns: b_video_ns,
                },
            ) => Self::I915Stats {
                gfx_ns: std::cmp::max(*a_gfx_ns, *b_gfx_ns),
                video_ns: std::cmp::max(*a_video_ns, *b_video_ns),
            },
            (
                Self::NvidiaStats {
                    gfx_percentage: a_gfx_percentage,
                    enc_percentage: a_enc_percentage,
                    dec_percentage: a_dec_percentage,
                    mem_bytes: a_mem_bytes,
                },
                Self::NvidiaStats {
                    gfx_percentage: b_gfx_percentage,
                    enc_percentage: b_enc_percentage,
                    dec_percentage: b_dec_percentage,
                    mem_bytes: b_mem_bytes,
                },
            ) => Self::NvidiaStats {
                gfx_percentage: std::cmp::max(*a_gfx_percentage, *b_gfx_percentage),
                enc_percentage: std::cmp::max(*a_enc_percentage, *b_enc_percentage),
                dec_percentage: std::cmp::max(*a_dec_percentage, *b_dec_percentage),
                mem_bytes: std::cmp::max(*a_mem_bytes, *b_mem_bytes),
            },
            (
                Self::V3dStats {
                    gfx_ns: a_gfx_ns,
                    mem_bytes: a_mem_bytes,
                },
                Self::V3dStats {
                    gfx_ns: b_gfx_ns,
                    mem_bytes: b_mem_bytes,
                },
            ) => Self::V3dStats {
                gfx_ns: std::cmp::max(*a_gfx_ns, *b_gfx_ns),
                mem_bytes: std::cmp::max(*a_mem_bytes, *b_mem_bytes),
            },
            (
                Self::XeStats {
                    gfx_cycles: a_gfx_cycles,
                    gfx_total_cycles: a_gfx_cycles_total,
                    video_cycles: a_video_cycles,
                    video_cycles_total: a_video_cycles_total,
                    mem_bytes: a_mem_bytes,
                },
                Self::XeStats {
                    gfx_cycles: b_gfx_cycles,
                    gfx_total_cycles: b_gfx_cycles_total,
                    video_cycles: b_video_cycles,
                    video_cycles_total: b_video_cycles_total,
                    mem_bytes: b_mem_bytes,
                },
            ) => Self::XeStats {
                gfx_cycles: std::cmp::max(*a_gfx_cycles, *b_gfx_cycles),
                gfx_total_cycles: std::cmp::max(*a_gfx_cycles_total, *b_gfx_cycles_total),
                video_cycles: std::cmp::max(*a_video_cycles, *b_video_cycles),
                video_cycles_total: std::cmp::max(*a_video_cycles_total, *b_video_cycles_total),
                mem_bytes: std::cmp::max(*a_mem_bytes, *b_mem_bytes),
            },
            _ => *self,
        }
    }
}
