use std::fmt::Display;

use nutype::nutype;
use serde::{Deserialize, Serialize};

use crate::pci_slot::PciSlot;

#[nutype(
    validate(greater_or_equal = 0, less_or_equal = 100),
    default = 0,
    derive(
        Debug,
        Default,
        Clone,
        Copy,
        Hash,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Serialize,
        Deserialize,
        FromStr,
        Deref,
        TryFrom,
        Display,
    )
)]
pub struct IntegerPercentage(u8);

impl IntegerPercentage {
    /// Returns the percentage as a fraction in `[0.0, 1.0]`.
    pub fn as_fraction(self) -> f32 {
        f32::from(self.into_inner()) / 100.0
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GpuIdentifier {
    PciSlot(PciSlot),
    Enumerator(usize),
}

impl Default for GpuIdentifier {
    fn default() -> Self {
        Self::Enumerator(usize::default())
    }
}

impl Display for GpuIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PciSlot(slot) => write!(f, "{slot}"),
            Self::Enumerator(n) => write!(f, "{n}"),
        }
    }
}

/// Raw counter snapshot for one GPU as seen by one process.
///
/// Usage fractions are computed by comparing two snapshots with [`gfx_fraction`] etc.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
        gfx_percentage: IntegerPercentage,
        enc_percentage: IntegerPercentage,
        dec_percentage: IntegerPercentage,
        mem_bytes: u64,
    },
    V3dStats {
        gfx_ns: u64,
        mem_bytes: u64,
    },
    XeStats {
        gfx_cycles: u64,
        gfx_total_cycles: u64,
        compute_cycles: u64,
        compute_total_cycles: u64,
        video_cycles: u64,
        video_total_cycles: u64,
        mem_bytes: u64,
    },
}

impl GpuUsageStats {
    // Fraction helpers

    /// `(new_ns - old_ns) / time_delta_ms` — clamped to `None` on zero delta.
    fn delta_ns(new: u64, old: u64, time_delta_ms: u64) -> Option<f32> {
        if time_delta_ms == 0 {
            return None;
        }
        Some(new.saturating_sub(old) as f32 / (time_delta_ms * 1_000_000) as f32)
    }

    /// Fraction of cycles used: `(new_cycles - old_cycles) / (new_total - old_total)`.
    fn delta_cycle_ratio(
        new_cycles: u64,
        old_cycles: u64,
        new_total: u64,
        old_total: u64,
    ) -> Option<f32> {
        let delta_total = new_total.saturating_sub(old_total) as f64;
        if delta_total == 0.0 {
            return None;
        }
        Some((new_cycles.saturating_sub(old_cycles) as f64 / delta_total) as f32)
    }

    fn max_options(a: Option<f32>, b: Option<f32>) -> Option<f32> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x.max(y)),
            _ => a.or(b),
        }
    }

    /// Graphics engine utilisation in `[0.0, 1.0]` relative to `old`.
    #[must_use]
    pub fn gfx_fraction(&self, old: &Self, time_delta_ms: u64) -> Option<f32> {
        match (self, old) {
            (Self::AmdgpuStats { gfx_ns: a, .. }, Self::AmdgpuStats { gfx_ns: b, .. })
            | (Self::I915Stats { gfx_ns: a, .. }, Self::I915Stats { gfx_ns: b, .. })
            | (Self::V3dStats { gfx_ns: a, .. }, Self::V3dStats { gfx_ns: b, .. }) => {
                Self::delta_ns(*a, *b, time_delta_ms)
            }

            (Self::NvidiaStats { gfx_percentage, .. }, Self::NvidiaStats { .. }) => {
                Some(gfx_percentage.as_fraction())
            }

            // xe reports cycles instead of ns and doesn't separate gfx/compute
            (
                Self::XeStats {
                    gfx_cycles: a_gfx,
                    gfx_total_cycles: a_gfx_total,
                    compute_cycles: a_cmp,
                    compute_total_cycles: a_cmp_total,
                    ..
                },
                Self::XeStats {
                    gfx_cycles: b_gfx,
                    gfx_total_cycles: b_gfx_total,
                    compute_cycles: b_cmp,
                    compute_total_cycles: b_cmp_total,
                    ..
                },
            ) => Self::max_options(
                Self::delta_cycle_ratio(*a_gfx, *b_gfx, *a_gfx_total, *b_gfx_total),
                Self::delta_cycle_ratio(*a_cmp, *b_cmp, *a_cmp_total, *b_cmp_total),
            ),

            _ => None,
        }
    }

    /// Video-encode engine utilisation in `[0.0, 1.0]` relative to `old`.
    #[must_use]
    pub fn enc_fraction(&self, old: &Self, time_delta_ms: u64) -> Option<f32> {
        match (self, old) {
            (Self::AmdgpuStats { enc_ns: a, .. }, Self::AmdgpuStats { enc_ns: b, .. })
            | (Self::I915Stats { video_ns: a, .. }, Self::I915Stats { video_ns: b, .. }) => {
                Self::delta_ns(*a, *b, time_delta_ms)
            }
            (Self::NvidiaStats { enc_percentage, .. }, Self::NvidiaStats { .. }) => {
                Some(enc_percentage.as_fraction())
            }
            (
                Self::XeStats {
                    video_cycles: a,
                    video_total_cycles: a_total,
                    ..
                },
                Self::XeStats {
                    video_cycles: b,
                    video_total_cycles: b_total,
                    ..
                },
            ) => Self::delta_cycle_ratio(*a, *b, *a_total, *b_total),
            _ => None,
        }
    }

    /// Video-decode engine utilisation in `[0.0, 1.0]` relative to `old`.
    ///
    /// For GPUs with a unified media engine this will equal [`enc_fraction`].
    /// Some AMD GPUs always return 0 for `dec_ns`.
    #[must_use]
    pub fn dec_fraction(&self, old: &Self, time_delta_ms: u64) -> Option<f32> {
        match (self, old) {
            (Self::AmdgpuStats { dec_ns: a, .. }, Self::AmdgpuStats { dec_ns: b, .. }) => {
                Self::delta_ns(*a, *b, time_delta_ms)
            }
            (Self::NvidiaStats { dec_percentage, .. }, Self::NvidiaStats { .. }) => {
                Some(dec_percentage.as_fraction())
            }
            (
                Self::XeStats {
                    video_cycles: a,
                    video_total_cycles: a_total,
                    ..
                },
                Self::XeStats {
                    video_cycles: b,
                    video_total_cycles: b_total,
                    ..
                },
            ) => Self::delta_cycle_ratio(*a, *b, *a_total, *b_total),
            _ => None,
        }
    }

    /// VRAM / GTT usage in bytes, or `None` if the driver doesn't report it.
    #[must_use]
    pub fn mem_bytes(&self) -> Option<u64> {
        match self {
            Self::AmdgpuStats { mem_bytes, .. }
            | Self::NvidiaStats { mem_bytes, .. }
            | Self::V3dStats { mem_bytes, .. }
            | Self::XeStats { mem_bytes, .. } => Some(*mem_bytes),
            Self::I915Stats { .. } => None,
        }
    }

    /// Component-wise maximum of two snapshots of the same variant.
    ///
    /// Returns `*self` unchanged if the variants differ (should not happen in practice).
    #[must_use]
    pub fn greater(&self, other: &Self) -> Self {
        match (self, other) {
            (
                Self::AmdgpuStats {
                    gfx_ns: a0,
                    enc_ns: a1,
                    dec_ns: a2,
                    mem_bytes: a3,
                },
                Self::AmdgpuStats {
                    gfx_ns: b0,
                    enc_ns: b1,
                    dec_ns: b2,
                    mem_bytes: b3,
                },
            ) => Self::AmdgpuStats {
                gfx_ns: *a0.max(b0),
                enc_ns: *a1.max(b1),
                dec_ns: *a2.max(b2),
                mem_bytes: *a3.max(b3),
            },

            (
                Self::I915Stats {
                    gfx_ns: a0,
                    video_ns: a1,
                },
                Self::I915Stats {
                    gfx_ns: b0,
                    video_ns: b1,
                },
            ) => Self::I915Stats {
                gfx_ns: *a0.max(b0),
                video_ns: *a1.max(b1),
            },

            (
                Self::NvidiaStats {
                    gfx_percentage: a0,
                    enc_percentage: a1,
                    dec_percentage: a2,
                    mem_bytes: a3,
                },
                Self::NvidiaStats {
                    gfx_percentage: b0,
                    enc_percentage: b1,
                    dec_percentage: b2,
                    mem_bytes: b3,
                },
            ) => Self::NvidiaStats {
                gfx_percentage: *a0.max(b0),
                enc_percentage: *a1.max(b1),
                dec_percentage: *a2.max(b2),
                mem_bytes: *a3.max(b3),
            },

            (
                Self::V3dStats {
                    gfx_ns: a0,
                    mem_bytes: a1,
                },
                Self::V3dStats {
                    gfx_ns: b0,
                    mem_bytes: b1,
                },
            ) => Self::V3dStats {
                gfx_ns: *a0.max(b0),
                mem_bytes: *a1.max(b1),
            },

            (
                Self::XeStats {
                    gfx_cycles: a0,
                    gfx_total_cycles: a1,
                    compute_cycles: a2,
                    compute_total_cycles: a3,
                    video_cycles: a4,
                    video_total_cycles: a5,
                    mem_bytes: a6,
                },
                Self::XeStats {
                    gfx_cycles: b0,
                    gfx_total_cycles: b1,
                    compute_cycles: b2,
                    compute_total_cycles: b3,
                    video_cycles: b4,
                    video_total_cycles: b5,
                    mem_bytes: b6,
                },
            ) => Self::XeStats {
                gfx_cycles: *a0.max(b0),
                gfx_total_cycles: *a1.max(b1),
                compute_cycles: *a2.max(b2),
                compute_total_cycles: *a3.max(b3),
                video_cycles: *a4.max(b4),
                video_total_cycles: *a5.max(b5),
                mem_bytes: *a6.max(b6),
            },

            _ => *self,
        }
    }
}
