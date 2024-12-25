use lazy_regex::{lazy_regex, Lazy, Regex};
use log::trace;

use super::IS_FLATPAK;

const PATH_OS_RELEASE: &str = "/etc/os-release";
const PATH_OS_RELEASE_FLATPAK: &str = "/run/host/etc/os-release";
const PATH_KERNEL_VERSION: &str = "/proc/sys/kernel/osrelease";

static RE_PRETTY_NAME: Lazy<Regex> = lazy_regex!("PRETTY_NAME=\"(.*)\"");

pub struct OsInfo {
    pub name: Option<String>,
    pub kernel_version: Option<String>,
}

impl OsInfo {
    pub fn get() -> Self {
        let os_path = if *IS_FLATPAK {
            PATH_OS_RELEASE_FLATPAK
        } else {
            PATH_OS_RELEASE
        };

        trace!("Path for the os-release file is determined to be `{os_path}`");

        let name = RE_PRETTY_NAME
            .captures(&std::fs::read_to_string(os_path).unwrap_or_default())
            .and_then(|captures| captures.get(1))
            .map(|capture| capture.as_str().trim().to_string());

        let kernel_version = std::fs::read_to_string(PATH_KERNEL_VERSION)
            .ok()
            .map(|s| s.trim().to_string());

        OsInfo {
            name,
            kernel_version,
        }
    }
}
