use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
};

use crate::{
    gpu_usage::{GpuIdentifier, GpuUsageStats},
    nvidia::NvidiaState,
};

#[derive(Debug, Clone)]
pub struct ProcessDataCache {
    non_gpu_fdinfos: Option<HashSet<(libc::pid_t, usize)>>,
    non_npu_fdinfos: Option<HashSet<(libc::pid_t, usize)>>,
    symlink_cache: Option<HashMap<(libc::pid_t, usize), PathBuf>>,
    nvidia_state: NvidiaState,
}

impl ProcessDataCache {
    pub fn new() -> Self {
        Self {
            non_gpu_fdinfos: Some(HashSet::default()),
            non_npu_fdinfos: Some(HashSet::default()),
            symlink_cache: Some(HashMap::default()),
            nvidia_state: NvidiaState::default(),
        }
    }

    pub fn new_no_fdinfo_cache() -> Self {
        Self {
            non_gpu_fdinfos: None,
            non_npu_fdinfos: None,
            symlink_cache: None,
            nvidia_state: NvidiaState::default(),
        }
    }

    pub(crate) fn non_gpu_fdinfos_contains(&self, pid: libc::pid_t, id: usize) -> bool {
        self.non_gpu_fdinfos
            .as_ref()
            .map(|set| set.contains(&(pid, id)))
            .unwrap_or_default()
    }

    pub(crate) fn non_gpu_fdinfos_insert(&mut self, pid: libc::pid_t, id: usize) -> bool {
        self.non_gpu_fdinfos
            .as_mut()
            .map(|set| set.insert((pid, id)))
            .unwrap_or(true)
    }

    pub(crate) fn non_gpu_fdinfos_remove(&mut self, pid: libc::pid_t, id: usize) -> bool {
        self.non_gpu_fdinfos
            .as_mut()
            .map(|set| set.remove(&(pid, id)))
            .unwrap_or_default()
    }

    pub(crate) fn non_npu_fdinfos_contains(&self, pid: libc::pid_t, id: usize) -> bool {
        self.non_npu_fdinfos
            .as_ref()
            .map(|set| set.contains(&(pid, id)))
            .unwrap_or_default()
    }

    pub(crate) fn non_npu_fdinfos_insert(&mut self, pid: libc::pid_t, id: usize) -> bool {
        self.non_npu_fdinfos
            .as_mut()
            .map(|set| set.insert((pid, id)))
            .unwrap_or(true)
    }

    pub(crate) fn non_npu_fdinfos_remove(&mut self, pid: libc::pid_t, id: usize) -> bool {
        self.non_npu_fdinfos
            .as_mut()
            .map(|set| set.remove(&(pid, id)))
            .unwrap_or_default()
    }

    pub(crate) fn symlink_cache_get(&self, pid: libc::pid_t, id: usize) -> Option<&PathBuf> {
        self.symlink_cache
            .as_ref()
            .and_then(|map| map.get(&(pid, id)))
    }

    pub(crate) fn symlink_cache_insert(
        &mut self,
        pid: libc::pid_t,
        id: usize,
        value: PathBuf,
    ) -> Option<PathBuf> {
        self.symlink_cache
            .as_mut()
            .and_then(|set| set.insert((pid, id), value))
    }

    pub(crate) fn nvidia_refresh(&mut self) {
        self.nvidia_state.refresh();
    }

    pub(crate) fn nvidia_get(&self, pid: libc::pid_t) -> BTreeMap<GpuIdentifier, GpuUsageStats> {
        self.nvidia_state.per_process_stats(pid)
    }
}
