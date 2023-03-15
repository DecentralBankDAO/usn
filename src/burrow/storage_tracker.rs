use crate::*;
use near_sdk::StorageUsage;

/// A helper object that tracks changes in state storage.
#[derive(Default)]
pub struct StorageTracker {
    pub bytes_added: StorageUsage,
    pub bytes_released: StorageUsage,
    pub initial_storage_usage: Option<StorageUsage>,
}

/// Safety guard for the storage tracker.
impl Drop for StorageTracker {
    fn drop(&mut self) {
        assert!(self.is_empty(), "Bug, non-tracked storage change");
    }
}

impl StorageTracker {
    /// Starts tracking the state storage changes.
    pub fn start(&mut self) {
        assert!(
            self.initial_storage_usage
                .replace(env::storage_usage())
                .is_none(),
            "The storage tracker is already tracking"
        );
    }

    /// Stop tracking the state storage changes and record changes in bytes.
    pub fn stop(&mut self) {
        let initial_storage_usage = self
            .initial_storage_usage
            .take()
            .expect("The storage tracker wasn't tracking");
        let storage_usage = env::storage_usage();
        if storage_usage >= initial_storage_usage {
            self.bytes_added += storage_usage - initial_storage_usage;
        } else {
            self.bytes_released += initial_storage_usage - storage_usage;
        }
    }

    /// Consumes the other storage tracker changes.
    pub fn consume(&mut self, other: &mut StorageTracker) {
        self.bytes_added += other.bytes_added;
        other.bytes_added = 0;
        self.bytes_released = other.bytes_released;
        other.bytes_released = 0;
        assert!(
            other.initial_storage_usage.is_none(),
            "Can't merge storage tracker that is tracking storage"
        );
    }

    /// Returns true if no bytes is added or released, and the tracker is not active.
    pub fn is_empty(&self) -> bool {
        self.bytes_added == 0 && self.bytes_released == 0 && self.initial_storage_usage.is_none()
    }
}
