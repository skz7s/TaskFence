use std::collections::BTreeMap;
use std::sync::Mutex;

use taskfence_core::{StateStore, TaskFenceError, TaskId, TaskStatus};

#[derive(Debug, Default)]
pub struct InMemoryStateStore {
    statuses: Mutex<BTreeMap<TaskId, TaskStatus>>,
}

impl InMemoryStateStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> taskfence_core::Result<BTreeMap<TaskId, TaskStatus>> {
        self.statuses
            .lock()
            .map(|statuses| statuses.clone())
            .map_err(|_| TaskFenceError::State("state store is poisoned".into()))
    }
}

impl StateStore for InMemoryStateStore {
    fn set_status(&self, task_id: &TaskId, status: TaskStatus) -> taskfence_core::Result<()> {
        self.statuses
            .lock()
            .map_err(|_| TaskFenceError::State("state store is poisoned".into()))?
            .insert(task_id.clone(), status);
        Ok(())
    }

    fn get_status(&self, task_id: &TaskId) -> taskfence_core::Result<Option<TaskStatus>> {
        self.statuses
            .lock()
            .map(|statuses| statuses.get(task_id).cloned())
            .map_err(|_| TaskFenceError::State("state store is poisoned".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_task_status_returns_none() {
        let store = InMemoryStateStore::new();

        assert_eq!(store.get_status(&TaskId("missing".into())).unwrap(), None);
    }

    #[test]
    fn set_and_get_status() {
        let store = InMemoryStateStore::new();
        let task_id = TaskId("task-1".into());

        store.set_status(&task_id, TaskStatus::Created).unwrap();
        store.set_status(&task_id, TaskStatus::Running).unwrap();

        assert_eq!(
            store.get_status(&task_id).unwrap(),
            Some(TaskStatus::Running)
        );
    }

    #[test]
    fn snapshot_returns_all_statuses() {
        let store = InMemoryStateStore::new();
        store
            .set_status(&TaskId("task-1".into()), TaskStatus::Succeeded)
            .unwrap();
        store
            .set_status(&TaskId("task-2".into()), TaskStatus::Denied)
            .unwrap();

        let snapshot = store.snapshot().unwrap();

        assert_eq!(snapshot.len(), 2);
        assert_eq!(
            snapshot.get(&TaskId("task-1".into())),
            Some(&TaskStatus::Succeeded)
        );
    }
}
