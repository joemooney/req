//! Background AI Evaluator Module
//!
//! Provides background evaluation of requirements to avoid blocking the UI.
//! Evaluates requirements one-by-one that either have no evaluation or
//! have been modified since their last evaluation.

use crate::ai::client::{AiClient, AiError};
use crate::ai::responses::StoredAiEvaluation;
use crate::models::RequirementsStore;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use uuid::Uuid;

/// Status of the background evaluator
#[derive(Debug, Clone, PartialEq)]
pub enum EvaluatorStatus {
    /// Evaluator is idle, waiting for work
    Idle,
    /// Currently evaluating a specific requirement
    Evaluating { req_id: Uuid, spec_id: String },
    /// Evaluator is paused
    Paused,
    /// Evaluator has been stopped
    Stopped,
    /// Evaluator encountered an error
    Error(String),
}

/// Result of an evaluation to be applied to the store
#[derive(Debug, Clone)]
pub struct EvaluationResult {
    pub req_id: Uuid,
    pub evaluation: StoredAiEvaluation,
}

/// Commands that can be sent to the evaluator
#[derive(Debug)]
pub enum EvaluatorCommand {
    /// Pause evaluation
    Pause,
    /// Resume evaluation
    Resume,
    /// Stop the evaluator thread
    Stop,
    /// Prioritize evaluating a specific requirement
    PrioritizeRequirement(Uuid),
    /// Trigger a scan for requirements needing evaluation
    ScanForWork,
}

/// Configuration for the background evaluator
#[derive(Debug, Clone)]
pub struct EvaluatorConfig {
    /// Delay between evaluations (to avoid rate limiting)
    pub evaluation_delay: Duration,
    /// Delay between scans when idle
    pub idle_scan_delay: Duration,
    /// Maximum evaluations per session (0 = unlimited)
    pub max_evaluations: usize,
}

impl Default for EvaluatorConfig {
    fn default() -> Self {
        Self {
            evaluation_delay: Duration::from_secs(5),
            idle_scan_delay: Duration::from_secs(60),
            max_evaluations: 0,
        }
    }
}

/// Background AI Evaluator
///
/// Runs in a background thread and evaluates requirements that need it.
pub struct BackgroundEvaluator {
    /// Handle to the background thread
    thread_handle: Option<JoinHandle<()>>,
    /// Channel to send commands to the evaluator
    command_tx: mpsc::Sender<EvaluatorCommand>,
    /// Channel to receive evaluation results
    result_rx: mpsc::Receiver<EvaluationResult>,
    /// Current status (shared with thread)
    status: Arc<Mutex<EvaluatorStatus>>,
    /// Flag to check if running
    running: Arc<AtomicBool>,
}

impl BackgroundEvaluator {
    /// Create and start a new background evaluator
    ///
    /// The evaluator will scan the store for requirements needing evaluation
    /// and evaluate them one by one.
    pub fn new(
        store: Arc<Mutex<RequirementsStore>>,
        ai_client: AiClient,
        config: EvaluatorConfig,
    ) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(EvaluatorStatus::Idle));
        let running = Arc::new(AtomicBool::new(true));

        let thread_status = Arc::clone(&status);
        let thread_running = Arc::clone(&running);

        let thread_handle = thread::spawn(move || {
            evaluator_thread(
                store,
                ai_client,
                config,
                command_rx,
                result_tx,
                thread_status,
                thread_running,
            );
        });

        Self {
            thread_handle: Some(thread_handle),
            command_tx,
            result_rx,
            status,
            running,
        }
    }

    /// Get the current status of the evaluator
    pub fn status(&self) -> EvaluatorStatus {
        self.status.lock().unwrap().clone()
    }

    /// Check if the evaluator is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Pause the evaluator
    pub fn pause(&self) -> Result<(), mpsc::SendError<EvaluatorCommand>> {
        self.command_tx.send(EvaluatorCommand::Pause)
    }

    /// Resume the evaluator
    pub fn resume(&self) -> Result<(), mpsc::SendError<EvaluatorCommand>> {
        self.command_tx.send(EvaluatorCommand::Resume)
    }

    /// Stop the evaluator
    pub fn stop(&self) -> Result<(), mpsc::SendError<EvaluatorCommand>> {
        self.command_tx.send(EvaluatorCommand::Stop)
    }

    /// Prioritize evaluating a specific requirement
    pub fn prioritize(&self, req_id: Uuid) -> Result<(), mpsc::SendError<EvaluatorCommand>> {
        self.command_tx
            .send(EvaluatorCommand::PrioritizeRequirement(req_id))
    }

    /// Trigger a scan for work
    pub fn scan(&self) -> Result<(), mpsc::SendError<EvaluatorCommand>> {
        self.command_tx.send(EvaluatorCommand::ScanForWork)
    }

    /// Try to receive a completed evaluation result
    pub fn try_recv_result(&self) -> Option<EvaluationResult> {
        self.result_rx.try_recv().ok()
    }

    /// Receive all pending evaluation results
    pub fn recv_all_results(&self) -> Vec<EvaluationResult> {
        let mut results = Vec::new();
        while let Ok(result) = self.result_rx.try_recv() {
            results.push(result);
        }
        results
    }
}

impl Drop for BackgroundEvaluator {
    fn drop(&mut self) {
        // Signal the thread to stop
        self.running.store(false, Ordering::SeqCst);
        let _ = self.command_tx.send(EvaluatorCommand::Stop);

        // Wait for the thread to finish
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

/// The main evaluator thread function
fn evaluator_thread(
    store: Arc<Mutex<RequirementsStore>>,
    ai_client: AiClient,
    config: EvaluatorConfig,
    command_rx: mpsc::Receiver<EvaluatorCommand>,
    result_tx: mpsc::Sender<EvaluationResult>,
    status: Arc<Mutex<EvaluatorStatus>>,
    running: Arc<AtomicBool>,
) {
    let mut paused = false;
    let mut priority_queue: Vec<Uuid> = Vec::new();
    let mut evaluation_count = 0;

    while running.load(Ordering::SeqCst) {
        // Process any pending commands
        while let Ok(cmd) = command_rx.try_recv() {
            match cmd {
                EvaluatorCommand::Pause => {
                    paused = true;
                    *status.lock().unwrap() = EvaluatorStatus::Paused;
                }
                EvaluatorCommand::Resume => {
                    paused = false;
                    *status.lock().unwrap() = EvaluatorStatus::Idle;
                }
                EvaluatorCommand::Stop => {
                    *status.lock().unwrap() = EvaluatorStatus::Stopped;
                    return;
                }
                EvaluatorCommand::PrioritizeRequirement(req_id) => {
                    if !priority_queue.contains(&req_id) {
                        priority_queue.insert(0, req_id);
                    }
                }
                EvaluatorCommand::ScanForWork => {
                    // Will scan on next iteration
                }
            }
        }

        if paused {
            thread::sleep(Duration::from_millis(100));
            continue;
        }

        // Check if we've hit the evaluation limit
        if config.max_evaluations > 0 && evaluation_count >= config.max_evaluations {
            *status.lock().unwrap() = EvaluatorStatus::Idle;
            thread::sleep(config.idle_scan_delay);
            continue;
        }

        // Find the next requirement to evaluate
        let next_req = find_next_requirement(&store, &mut priority_queue);

        match next_req {
            Some((req_id, spec_id, req_clone, store_clone)) => {
                // Update status
                *status.lock().unwrap() = EvaluatorStatus::Evaluating {
                    req_id,
                    spec_id: spec_id.clone(),
                };

                eprintln!(
                    "DEBUG: Background evaluator starting evaluation of {}",
                    spec_id
                );

                // Perform the evaluation
                match ai_client.evaluate_requirement(&req_clone, &store_clone) {
                    Ok(eval_response) => {
                        let content_hash = req_clone.content_hash();
                        let stored_eval = StoredAiEvaluation::new(eval_response, content_hash);

                        let result = EvaluationResult {
                            req_id,
                            evaluation: stored_eval,
                        };

                        if result_tx.send(result).is_err() {
                            // Receiver dropped, stop the thread
                            return;
                        }

                        evaluation_count += 1;
                        eprintln!(
                            "DEBUG: Background evaluator completed evaluation of {}",
                            spec_id
                        );
                    }
                    Err(e) => {
                        eprintln!(
                            "DEBUG: Background evaluator failed to evaluate {}: {}",
                            spec_id, e
                        );
                        match e {
                            AiError::RateLimited => {
                                // Wait longer before retrying
                                thread::sleep(Duration::from_secs(30));
                            }
                            AiError::NotAvailable => {
                                // AI not available, stop trying
                                *status.lock().unwrap() =
                                    EvaluatorStatus::Error("AI not available".to_string());
                                thread::sleep(config.idle_scan_delay);
                            }
                            _ => {
                                // Other error, continue with delay
                                *status.lock().unwrap() = EvaluatorStatus::Error(e.to_string());
                            }
                        }
                    }
                }

                // Delay between evaluations
                thread::sleep(config.evaluation_delay);
            }
            None => {
                // No work to do, go idle
                *status.lock().unwrap() = EvaluatorStatus::Idle;
                thread::sleep(config.idle_scan_delay);
            }
        }
    }

    *status.lock().unwrap() = EvaluatorStatus::Stopped;
}

/// Find the next requirement that needs evaluation
fn find_next_requirement(
    store: &Arc<Mutex<RequirementsStore>>,
    priority_queue: &mut Vec<Uuid>,
) -> Option<(Uuid, String, crate::models::Requirement, RequirementsStore)> {
    let store_guard = store.lock().ok()?;

    // First, check the priority queue
    while let Some(req_id) = priority_queue.first().copied() {
        priority_queue.remove(0);
        if let Some(req) = store_guard.requirements.iter().find(|r| r.id == req_id) {
            if req.needs_ai_evaluation() {
                let spec_id = req.spec_id.clone().unwrap_or_else(|| req.id.to_string());
                return Some((req.id, spec_id, req.clone(), store_guard.clone()));
            }
        }
    }

    // Then, find any requirement that needs evaluation
    for req in &store_guard.requirements {
        if req.needs_ai_evaluation() {
            let spec_id = req.spec_id.clone().unwrap_or_else(|| req.id.to_string());
            return Some((req.id, spec_id, req.clone(), store_guard.clone()));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluator_config_default() {
        let config = EvaluatorConfig::default();
        assert_eq!(config.evaluation_delay, Duration::from_secs(5));
        assert_eq!(config.idle_scan_delay, Duration::from_secs(60));
        assert_eq!(config.max_evaluations, 0);
    }

    #[test]
    fn test_evaluator_status_eq() {
        assert_eq!(EvaluatorStatus::Idle, EvaluatorStatus::Idle);
        assert_eq!(EvaluatorStatus::Paused, EvaluatorStatus::Paused);
        assert_ne!(EvaluatorStatus::Idle, EvaluatorStatus::Paused);
    }
}
