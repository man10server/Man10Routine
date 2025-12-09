use std::collections::{HashMap, VecDeque};

use futures::future::BoxFuture;
use thiserror::Error;
use tokio::task::JoinSet;
use tracing::{Instrument, instrument};

use crate::error::{SpannedErr, SpannedExt};

use super::shutdown::Shutdown;

pub type TaskFuture<E> = BoxFuture<'static, Result<(), E>>;
pub type TaskFn<TCtx, E> = Box<dyn Fn(TCtx) -> TaskFuture<E> + Send + 'static>;

pub struct TaskSpec<TCtx, E> {
    pub name: String,
    pub deps: Vec<String>,
    pub exec: TaskFn<TCtx, E>,
}

impl<TCtx, E> TaskSpec<TCtx, E> {
    pub fn new(
        name: impl Into<String>,
        deps: impl Into<Vec<String>>,
        exec: impl Fn(TCtx) -> TaskFuture<E> + Send + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            deps: deps.into(),
            exec: Box::new(exec),
        }
    }
}

pub struct Scheduler<TCtx, E> {
    tasks: HashMap<String, TaskSpec<TCtx, E>>,
    reverse_edges: HashMap<String, Vec<String>>,
    indegree: HashMap<String, usize>,
    shutdown: Shutdown,
}

#[derive(Error, Debug)]
pub enum InvalidDagError {
    #[error("Duplicate task name detected: {0}")]
    DuplicateTaskName(String),

    #[error("Task '{task}' depends on unknown task '{dependency}'")]
    UnknownDependency { task: String, dependency: String },
}

impl<TCtx, E> Scheduler<TCtx, E>
where
    TCtx: Clone + Send + 'static,
    E: Send + 'static,
{
    #[instrument(skip(tasks, shutdown))]
    pub fn from_tasks(
        tasks: Vec<TaskSpec<TCtx, E>>,
        shutdown: Shutdown,
    ) -> Result<Self, SpannedErr<InvalidDagError>> {
        let mut tasks_map: HashMap<String, TaskSpec<TCtx, E>> = HashMap::new();
        for task in tasks.into_iter() {
            if tasks_map.contains_key(&task.name) {
                return Err(InvalidDagError::DuplicateTaskName(task.name)).with_span_trace();
            }
            tasks_map.insert(task.name.clone(), task);
        }

        let mut indegree: HashMap<String, usize> = HashMap::new();
        let mut reverse_edges: HashMap<String, Vec<String>> = HashMap::new();

        for task in tasks_map.values() {
            indegree.entry(task.name.clone()).or_insert(0);
            for dep in &task.deps {
                if !tasks_map.contains_key(dep) {
                    return Err(InvalidDagError::UnknownDependency {
                        task: task.name.clone(),
                        dependency: dep.clone(),
                    })
                    .with_span_trace();
                }
                reverse_edges
                    .entry(dep.clone())
                    .or_default()
                    .push(task.name.clone());
                *indegree.entry(task.name.clone()).or_insert(0) += 1;
            }
        }

        Ok(Scheduler {
            tasks: tasks_map,
            reverse_edges,
            indegree,
            shutdown,
        })
    }

    pub async fn run(mut self, ctx: TCtx) -> Result<Result<(), E>, tokio::task::JoinError> {
        let mut ready: VecDeque<String> = self
            .indegree
            .iter()
            .filter_map(|(name, deg)| if *deg == 0 { Some(name.clone()) } else { None })
            .collect();

        let mut inflight: JoinSet<(String, Result<(), E>)> = JoinSet::new();

        while !ready.is_empty() || !inflight.is_empty() {
            if self.shutdown.requested() {
                ready.clear();
            }

            while let Some(task_name) = ready.pop_front() {
                if self.shutdown.requested() {
                    break;
                }

                let task_spec = self.tasks.remove(&task_name).expect("task must exist");
                let exec = task_spec.exec;
                let ctx_clone = ctx.clone();
                inflight.spawn(
                    async move {
                        let res = exec(ctx_clone).await;
                        (task_name, res)
                    }
                    .in_current_span(),
                );
            }

            let Some(joined) = inflight.join_next().await else {
                continue;
            };

            match joined {
                Ok((name, res)) => {
                    if let Err(e) = res {
                        return Ok(Err(e));
                    }

                    if let Some(dependents) = self.reverse_edges.get(&name) {
                        for dependent_name in dependents {
                            let entry = self
                                .indegree
                                .get_mut(dependent_name)
                                .expect("indegree should exist for dependent task");
                            *entry -= 1;
                            if *entry == 0 && !self.shutdown.requested() {
                                ready.push_back(dependent_name.clone());
                            }
                        }
                    }
                }
                Err(join_err) => return Err(join_err),
            }
        }

        Ok(Ok(()))
    }
}
