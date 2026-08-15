use crate::command_error::CommandError;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct TaskRegistry {
    tasks: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl TaskRegistry {
    pub fn register(&self, task_id: String) -> Result<TaskLease, CommandError> {
        let cancellation = Arc::new(AtomicBool::new(false));
        let mut tasks = self.tasks.lock().unwrap_or_else(|error| error.into_inner());
        if tasks.contains_key(&task_id) {
            return Err(CommandError::new(
                "task.conflict",
                format!("Task {task_id} is already running"),
            ));
        }
        tasks.insert(task_id.clone(), Arc::clone(&cancellation));
        Ok(TaskLease {
            task_id,
            cancellation,
            registry: self.clone(),
        })
    }

    pub fn cancel(&self, task_id: &str) -> bool {
        let tasks = self.tasks.lock().unwrap_or_else(|error| error.into_inner());
        let Some(cancellation) = tasks.get(task_id) else {
            return false;
        };
        cancellation.store(true, Ordering::Release);
        true
    }

    fn remove(&self, task_id: &str) {
        self.tasks
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(task_id);
    }
}

pub struct TaskLease {
    task_id: String,
    cancellation: Arc<AtomicBool>,
    registry: TaskRegistry,
}

impl TaskLease {
    pub fn cancellation(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancellation)
    }
}

impl Drop for TaskLease {
    fn drop(&mut self) {
        self.registry.remove(&self.task_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_cancels_and_releases_task_ids() {
        let registry = TaskRegistry::default();
        let lease = registry
            .register("migration-1".to_owned())
            .expect("register");
        assert!(registry.register("migration-1".to_owned()).is_err());
        assert!(registry.cancel("migration-1"));
        assert!(lease.cancellation.load(Ordering::Acquire));
        drop(lease);
        assert!(!registry.cancel("migration-1"));
        assert!(registry.register("migration-1".to_owned()).is_ok());
    }
}
