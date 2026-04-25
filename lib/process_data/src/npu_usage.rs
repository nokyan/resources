use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum NpuUsageStats {
    AmdxdnaStats { usage_ns: u64, mem_bytes: u64 },
}

impl NpuUsageStats {
    fn delta_ns(new: u64, old: u64, time_delta_ms: u64) -> Option<f32> {
        if time_delta_ms == 0 {
            return None;
        }
        Some(new.saturating_sub(old) as f32 / (time_delta_ms * 1_000_000) as f32)
    }

    /// NPU engine utilisation in `[0.0, 1.0]` relative to `old`
    pub fn usage_fraction(&self, old: &Self, time_delta_ms: u64) -> Option<f32> {
        match (self, old) {
            (Self::AmdxdnaStats { usage_ns: a, .. }, Self::AmdxdnaStats { usage_ns: b, .. }) => {
                Self::delta_ns(*a, *b, time_delta_ms)
            }
        }
    }

    /// Memory usage in bytes, or `None` if unavailable
    pub fn mem_bytes(&self) -> Option<u64> {
        match self {
            Self::AmdxdnaStats { mem_bytes, .. } => Some(*mem_bytes),
        }
    }

    /// Component-wise maximum of two snapshots of the same variant
    pub fn greater(&self, other: &Self) -> Self {
        match (self, other) {
            (
                Self::AmdxdnaStats {
                    usage_ns: a_ns,
                    mem_bytes: a_mem_bytes,
                },
                Self::AmdxdnaStats {
                    usage_ns: b_ns,
                    mem_bytes: b_mem_bytes,
                },
            ) => Self::AmdxdnaStats {
                usage_ns: *a_ns.max(b_ns),
                mem_bytes: *a_mem_bytes.max(b_mem_bytes),
            },
        }
    }
}
