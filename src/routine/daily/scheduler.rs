use std::collections::{HashMap, VecDeque};

use futures::future::BoxFuture;
use tokio::task::JoinSet;
use tracing::warn;

use super::DailyRoutineContext;
use crate::routine::daily::error::DailyRoutineError;
use crate::routine::daily::shutdown::Shutdown;

pub(crate) type TaskFuture = BoxFuture<'static, Result<(), DailyRoutineError>>;
pub(crate) type TaskFn = fn(DailyRoutineContext) -> TaskFuture;

#[derive(Clone, Copy)]
pub(crate) struct TaskSpec {
    pub(crate) name: &'static str,
    pub(crate) deps: &'static [&'static str],
    pub(crate) exec: TaskFn,
}

impl TaskSpec {
    pub(crate) fn new(name: &'static str, deps: &'static [&'static str], exec: TaskFn) -> Self {
        Self { name, deps, exec }
    }
}

pub(crate) struct Scheduler {
    tasks: HashMap<&'static str, TaskSpec>,
    reverse_edges: HashMap<&'static str, Vec<&'static str>>,
    indegree: HashMap<&'static str, usize>,
    shutdown: Shutdown,
}

impl Scheduler {
    pub(crate) fn new(tasks: Vec<TaskSpec>, shutdown: Shutdown) -> Self {
        let mut tasks_map: HashMap<&'static str, TaskSpec> = HashMap::new();
        for task in tasks {
            if tasks_map.contains_key(task.name) {
                panic!("Duplicate task name detected: {}", task.name);
            }
            tasks_map.insert(task.name, task);
        }

        let mut indegree: HashMap<&'static str, usize> = HashMap::new();
        let mut reverse_edges: HashMap<&'static str, Vec<&'static str>> = HashMap::new();

        for task in tasks_map.values() {
            indegree.entry(task.name).or_insert(0);
            for dep in task.deps {
                if !tasks_map.contains_key(dep) {
                    panic!("Task '{}' depends on unknown task '{}'", task.name, dep);
                }
                reverse_edges.entry(dep).or_default().push(task.name);
                *indegree.entry(task.name).or_insert(0) += 1;
            }
        }

        Scheduler {
            tasks: tasks_map,
            reverse_edges,
            indegree,
            shutdown,
        }
    }

    pub(crate) async fn run(mut self, ctx: DailyRoutineContext) -> Result<(), DailyRoutineError> {
        let mut ready: VecDeque<&'static str> = self
            .indegree
            .iter()
            .filter_map(|(name, deg)| if *deg == 0 { Some(*name) } else { None })
            .collect();

        let mut inflight: JoinSet<(&'static str, Result<(), DailyRoutineError>)> = JoinSet::new();

        while !ready.is_empty() || !inflight.is_empty() {
            if self.shutdown.requested() {
                // Stop scheduling new tasks once shutdown is requested.
                ready.clear();
            }

            while let Some(task_name) = ready.pop_front() {
                if self.shutdown.requested() {
                    break;
                }

                let exec = self.tasks.get(task_name).expect("task must exist").exec;
                let ctx_clone = ctx.clone();
                inflight.spawn(async move { (task_name, exec(ctx_clone).await) });
            }

            let Some(joined) = inflight.join_next().await else {
                continue;
            };

            match joined {
                Ok((name, res)) => {
                    res?;

                    if let Some(dependents) = self.reverse_edges.get(name) {
                        for dependent_name in dependents {
                            let entry = self
                                .indegree
                                .get_mut(dependent_name)
                                .expect("indegree should exist for dependent task");
                            *entry -= 1;
                            if *entry == 0 && !self.shutdown.requested() {
                                ready.push_back(*dependent_name);
                            }
                        }
                    }
                }
                Err(join_err) => {
                    warn!("A task panicked or was cancelled: {}", join_err);
                    return Err(DailyRoutineError::TaskJoin(join_err));
                }
            }
        }

        Ok(())
    }
}
