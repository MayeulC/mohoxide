use std::sync::Arc;

use crate::engine::vfs::tree::FsDir;

pub struct DebugUiState {
    pub vfs_tree: Option<Arc<FsDir>>,
}

impl DebugUiState {
    pub fn new() -> Self {
        Self { vfs_tree: None }
    }
}
