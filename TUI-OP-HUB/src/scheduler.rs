use crate::error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use cron::Schedule;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Scheduled workflow task
#[derive(Debug, Clone)]
pub struct ScheduledTask {
    pub id: String,
    pub workflow_id: String,
    pub cron_expr: String,
    pub schedule: Schedule,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: DateTime<Utc>,
    pub enabled: bool,
}

/// Validate a cron expression and return the **normalized** expression plus
/// the parsed schedule.
///
/// Accepts classic 5-field crontab syntax (`0 9 * * MON`), the `cron` crate's
/// 6/7-field syntax with seconds, and the `@hourly`/`@daily`/... aliases.
/// Classic 5-field input is normalized by prepending a `0` seconds field, so
/// the stored expression always reloads cleanly in the scheduler daemon.
pub fn validate_cron(expr: &str) -> AppResult<(String, Schedule)> {
    let trimmed = expr.trim();
    let normalized = if trimmed.starts_with('@') || trimmed.split_whitespace().count() >= 6 {
        trimmed.to_string()
    } else {
        format!("0 {trimmed}")
    };
    let schedule = Schedule::from_str(&normalized)
        .map_err(|e| AppError::Validation(format!("Invalid cron expression: {}", e)))?;
    Ok((normalized, schedule))
}

/// Workflow scheduler daemon
pub struct WorkflowScheduler {
    pool: Arc<SqlitePool>,
    schedules: Arc<RwLock<HashMap<String, ScheduledTask>>>,
    running: Arc<AtomicBool>,
}

impl WorkflowScheduler {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self {
            pool,
            schedules: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the scheduler daemon
    pub async fn start(&self) -> AppResult<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(AppError::Other("Scheduler already running".into()));
        }

        // Load all scheduled tasks from database
        self.load_schedules().await?;

        let pool = self.pool.clone();
        let schedules = self.schedules.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            tracing::info!("Workflow scheduler started");

            while running.load(Ordering::Relaxed) {
                let now = Utc::now();

                // Get due workflows
                let due_tasks = {
                    let schedules_read = schedules.read().await;
                    schedules_read
                        .values()
                        .filter(|task| task.enabled && task.next_run <= now)
                        .cloned()
                        .collect::<Vec<_>>()
                };

                // Execute due workflows
                for task in due_tasks {
                    let pool_clone = pool.clone();
                    let schedules_clone = schedules.clone();
                    let task_clone = task.clone();

                    tokio::spawn(async move {
                        if let Err(e) =
                            execute_scheduled_workflow(pool_clone.clone(), &task_clone).await
                        {
                            tracing::error!(
                                workflow_id = %task_clone.workflow_id,
                                error = %e,
                                "Failed to execute scheduled workflow"
                            );
                        }

                        // Update next run time
                        if let Err(e) =
                            update_next_run(pool_clone, schedules_clone, &task_clone).await
                        {
                            tracing::error!(
                                task_id = %task_clone.id,
                                error = %e,
                                "Failed to update next run time"
                            );
                        }
                    });
                }

                // Sleep for 1 minute before next check
                tokio::time::sleep(Duration::from_secs(60)).await;
            }

            tracing::info!("Workflow scheduler stopped");
        });

        Ok(())
    }

    /// Stop the scheduler daemon
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Load all scheduled tasks from database
    async fn load_schedules(&self) -> AppResult<()> {
        let tasks = sqlx::query(
            r#"
            SELECT id, workflow_id, cron_expr, last_run, next_run, enabled
            FROM scheduled_tasks
            WHERE enabled = 1
            "#,
        )
        .fetch_all(&*self.pool)
        .await?;

        let mut schedules = self.schedules.write().await;
        schedules.clear();

        for task in tasks {
            let id: String = task.try_get("id")?;
            let workflow_id: String = task.try_get("workflow_id")?;
            let cron_expr: String = task.try_get("cron_expr")?;
            let last_run: Option<String> = task.try_get("last_run").ok();
            let next_run: String = task.try_get("next_run")?;

            let schedule = Schedule::from_str(&cron_expr)
                .map_err(|e| AppError::Validation(format!("Invalid cron expression: {}", e)))?;

            let last_run_dt = last_run.as_deref().and_then(|s| {
                DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            });
            let next_run_dt = DateTime::parse_from_rfc3339(&next_run)
                .map_err(|e| AppError::Other(format!("Invalid next_run timestamp: {}", e)))?
                .with_timezone(&Utc);

            let scheduled_task = ScheduledTask {
                id: id.clone(),
                workflow_id,
                cron_expr,
                schedule,
                last_run: last_run_dt,
                next_run: next_run_dt,
                enabled: true,
            };

            schedules.insert(id, scheduled_task);
        }

        tracing::info!(count = schedules.len(), "Loaded scheduled tasks");
        Ok(())
    }

    /// Add a new scheduled task
    pub async fn add_schedule(&self, workflow_id: String, cron_expr: String) -> AppResult<String> {
        let schedule = Schedule::from_str(&cron_expr)
            .map_err(|e| AppError::Validation(format!("Invalid cron expression: {}", e)))?;

        let next_run = schedule
            .upcoming(Utc)
            .next()
            .ok_or_else(|| AppError::Other("No upcoming schedule".into()))?;

        let id = Uuid::new_v4().to_string();

        // Save to database
        sqlx::query(
            r#"
            INSERT INTO scheduled_tasks (id, workflow_id, cron_expr, next_run, enabled)
            VALUES (?, ?, ?, ?, 1)
            "#,
        )
        .bind(&id)
        .bind(&workflow_id)
        .bind(&cron_expr)
        .bind(next_run.to_rfc3339())
        .execute(&*self.pool)
        .await?;

        // Add to in-memory schedules
        let scheduled_task = ScheduledTask {
            id: id.clone(),
            workflow_id,
            cron_expr,
            schedule,
            last_run: None,
            next_run,
            enabled: true,
        };

        self.schedules
            .write()
            .await
            .insert(id.clone(), scheduled_task);

        tracing::info!(task_id = %id, "Added scheduled task");
        Ok(id)
    }

    /// Remove a scheduled task
    pub async fn remove_schedule(&self, task_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM scheduled_tasks WHERE id = ?")
            .bind(task_id)
            .execute(&*self.pool)
            .await?;

        self.schedules.write().await.remove(task_id);

        tracing::info!(task_id = %task_id, "Removed scheduled task");
        Ok(())
    }

    /// Enable/disable a scheduled task
    pub async fn set_enabled(&self, task_id: &str, enabled: bool) -> AppResult<()> {
        let enabled_int = if enabled { 1 } else { 0 };

        sqlx::query("UPDATE scheduled_tasks SET enabled = ? WHERE id = ?")
            .bind(enabled_int)
            .bind(task_id)
            .execute(&*self.pool)
            .await?;

        if let Some(task) = self.schedules.write().await.get_mut(task_id) {
            task.enabled = enabled;
        }

        tracing::info!(task_id = %task_id, enabled = enabled, "Updated task status");
        Ok(())
    }

    /// List all scheduled tasks
    pub async fn list_schedules(&self) -> Vec<ScheduledTask> {
        self.schedules.read().await.values().cloned().collect()
    }
}

/// Execute a scheduled workflow
async fn execute_scheduled_workflow(pool: Arc<SqlitePool>, task: &ScheduledTask) -> AppResult<()> {
    tracing::info!(
        workflow_id = %task.workflow_id,
        task_id = %task.id,
        "Executing scheduled workflow"
    );

    // Execute workflow by ID
    let result = crate::workflow::execute_workflow_by_id(pool, &task.workflow_id, None).await;

    // Log result
    match &result {
        Ok(_) => {
            tracing::info!(
                workflow_id = %task.workflow_id,
                "Scheduled workflow completed successfully"
            );
        }
        Err(e) => {
            tracing::error!(
                workflow_id = %task.workflow_id,
                error = %e,
                "Scheduled workflow failed"
            );
        }
    }

    result.map(|_| ())
}

/// Update the next run time for a task
async fn update_next_run(
    pool: Arc<SqlitePool>,
    schedules: Arc<RwLock<HashMap<String, ScheduledTask>>>,
    task: &ScheduledTask,
) -> AppResult<()> {
    let now = Utc::now();
    let next_run = task
        .schedule
        .after(&now)
        .next()
        .ok_or_else(|| AppError::Other("No next run time".into()))?;

    // Update database
    sqlx::query("UPDATE scheduled_tasks SET last_run = ?, next_run = ? WHERE id = ?")
        .bind(now.to_rfc3339())
        .bind(next_run.to_rfc3339())
        .bind(&task.id)
        .execute(&*pool)
        .await?;

    // Update in-memory schedule
    if let Some(scheduled_task) = schedules.write().await.get_mut(&task.id) {
        scheduled_task.last_run = Some(now);
        scheduled_task.next_run = next_run;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_parsing() {
        // Note: the `cron` crate expects a seconds field (6 or 7 fields).

        // Every minute
        let schedule = Schedule::from_str("* * * * * *").unwrap();
        let next = schedule.upcoming(Utc).next().unwrap();
        assert!(next > Utc::now());

        // Every hour (at second 0)
        let schedule = Schedule::from_str("0 0 * * * *").unwrap();
        let next = schedule.upcoming(Utc).next().unwrap();
        assert!(next > Utc::now());

        // Daily at midnight
        let schedule = Schedule::from_str("0 0 0 * * *").unwrap();
        let next = schedule.upcoming(Utc).next().unwrap();
        assert!(next > Utc::now());
    }
}
