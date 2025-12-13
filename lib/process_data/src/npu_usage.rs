use serde::{Deserialize, Serialize};

/// Represents NPU usage statistics per-process.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub enum NpuUsageStats {
    AmdxdnaStats { usage_ns: u64, mem_bytes: u64 },
}

impl NpuUsageStats {
    fn delta_ns(a: u64, b: u64, time_delta: u64) -> Option<f32> {
        if time_delta == 0 {
            None
        } else {
            Some(a.saturating_sub(b) as f32 / (time_delta * 1_000_000) as f32)
        }
    }

    pub fn usage_fraction(&self, old: &Self, time_delta: u64) -> Option<f32> {
        match (self, old) {
            (
                Self::AmdxdnaStats { usage_ns: a_ns, .. },
                Self::AmdxdnaStats { usage_ns: b_ns, .. },
            ) => Self::delta_ns(*a_ns, *b_ns, time_delta),
        }
    }

    pub fn mem(&self) -> Option<u64> {
        match self {
            Self::AmdxdnaStats { mem_bytes, .. } => Some(*mem_bytes),
        }
    }

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
