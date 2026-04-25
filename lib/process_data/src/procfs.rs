use anyhow::{Context, Result};
use lazy_regex::{Lazy, Regex, lazy_regex};

/// Collection of parsers for procfs shenanigans

static RE_UID: Lazy<Regex> = lazy_regex!(r"Uid:\s*(\d+)");
static RE_AFFINITY: Lazy<Regex> = lazy_regex!(r"Cpus_allowed:\s*([0-9A-Fa-f]+)");
static RE_MEM: Lazy<Regex> = lazy_regex!(r"VmRSS:\s*([0-9]+)\s*kB");
static RE_SWAP: Lazy<Regex> = lazy_regex!(r"VmSwap:\s*([0-9]+)\s*kB");

static RE_CGROUP: Lazy<Regex> = lazy_regex!(
    r"(?U)/(?:app|background)\.slice/(?:app-|dbus-:)(?:(?P<launcher>[^-]+)-)?(?P<cgroup>[^-]+)(?:-[0-9]+|@[0-9]+)?\.(?:scope|service)"
);

static RE_READ: Lazy<Regex> = lazy_regex!(r"read_bytes:\s*(\d+)");
static RE_WRITE: Lazy<Regex> = lazy_regex!(r"write_bytes:\s*(\d+)");

pub fn parse_read_bytes(io: &str) -> Option<u64> {
    parse_field(io, &RE_READ)
}

pub fn parse_write_bytes(io: &str) -> Option<u64> {
    parse_field(io, &RE_WRITE)
}

fn parse_field(io: &str, re: &Regex) -> Option<u64> {
    re.captures(io)?.get(1)?.as_str().parse().ok()
}

/// Parse the real UID from `/proc/<pid>/status`.
pub fn parse_uid(status: &str) -> Result<u32> {
    RE_UID
        .captures(status)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().parse::<u32>().context("uid: parse error"))
        .unwrap_or(Ok(0))
}

/// Parse `Cpus_allowed` into a `Vec<bool>` of length `num_cpus`.
pub fn parse_affinity(status: &str, num_cpus: usize) -> Vec<bool> {
    let hex = RE_AFFINITY
        .captures(status)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
        .unwrap_or_default();

    let mut affinity = Vec::with_capacity(num_cpus);
    for nibble in hex.chars().filter_map(|c| c.to_digit(16)).rev() {
        for bit in 0..4u32 {
            if affinity.len() >= num_cpus {
                break;
            }
            affinity.push((nibble & (1 << bit)) != 0);
        }
    }
    affinity
}

/// Parse `VmRSS` in bytes (kB → bytes).
pub fn parse_memory_usage(status: &str) -> usize {
    parse_kb(status, &RE_MEM)
}

/// Parse `VmSwap` in bytes (kB → bytes).
pub fn parse_swap_usage(status: &str) -> usize {
    parse_kb(status, &RE_SWAP)
}

fn parse_kb(status: &str, re: &Regex) -> usize {
    re.captures(status)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<usize>().ok())
        .unwrap_or(0)
        .saturating_mul(1024)
}

pub fn parse_stat(raw: &str) -> Result<Vec<&str>> {
    let after_comm = raw
        .split(')')
        .next_back()
        .context("stat: missing closing ')'")?;

    // The first character after ')' is a space; skip it.
    Ok(after_comm.split(' ').skip(1).collect())
}

/// Extract `(launcher, app_id)` from the content of `/proc/<pid>/cgroup`.
///
/// Returns `(None, None)` if the cgroup doesn't match the expected format.
pub fn sanitize(raw: &str) -> (Option<String>, Option<String>) {
    RE_CGROUP
        .captures(raw)
        .map(|c| {
            (
                c.name("launcher")
                    .and_then(|m| decode_hex_escapes(m.as_str()).ok()),
                c.name("cgroup")
                    .and_then(|m| decode_hex_escapes(m.as_str()).ok()),
            )
        })
        .unwrap_or_default()
}

/// Decode `\xNN` escape sequences embedded in cgroup names.
///
/// Some applications (e.g. Mullvad) use this encoding to include `-` in their
/// cgroup name even though that character is not normally valid there.
fn decode_hex_escapes(s: &str) -> Result<String, ()> {
    #[inline]
    const fn from_hex(b: u8) -> Result<u8, ()> {
        match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            b'A'..=b'F' => Ok(b - b'A' + 10),
            _ => Err(()),
        }
    }

    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() && bytes[i + 1] == b'x' {
            let val = (from_hex(bytes[i + 2])? << 4) | from_hex(bytes[i + 3])?;
            out.push(val);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }

    String::from_utf8(out).map_err(|_| ())
}
