use blueprint_sdk::{
    tangle::layers::TangleLayer,
    testing::utils::tangle::{
        multi_node::MultiNodeTestEnv, runner::MockHeartbeatConsumer, TangleTestHarness,
    },
    Job,
};
use color_eyre::eyre::eyre;
use color_eyre::Result;
use faas_blueprint_lib::{
    context::FaaSContext,
    jobs::{
        create_branch_job, create_snapshot_job, execute_advanced_job, execute_function_job,
        expose_port_job, merge_branches_job, pause_instance_job, restore_snapshot_job,
        resume_instance_job, start_instance_job, stop_instance_job, upload_files_job,
        CREATE_BRANCH_JOB_ID, CREATE_SNAPSHOT_JOB_ID, EXECUTE_ADVANCED_JOB_ID,
        EXECUTE_FUNCTION_JOB_ID, EXPOSE_PORT_JOB_ID, MERGE_BRANCHES_JOB_ID, PAUSE_INSTANCE_JOB_ID,
        RESTORE_SNAPSHOT_JOB_ID, RESUME_INSTANCE_JOB_ID, START_INSTANCE_JOB_ID,
        STOP_INSTANCE_JOB_ID, UPLOAD_FILES_JOB_ID,
    },
};
use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

const USURPED_NONCE_MSG: &str = "Transaction was usurped by another with the same nonce";
pub async fn setup_services_with_retry<Ctx, const N: usize>(
    harness: &TangleTestHarness<Ctx>,
    exit_after_registration: bool,
) -> Result<(MultiNodeTestEnv<Ctx, MockHeartbeatConsumer>, u64, u64)>
where
    Ctx: Clone + Send + Sync + 'static,
{
    std::env::set_var("FAAS_DISABLE_PREWARM", "1");
    std::env::set_var("FAAS_ENABLE_CONTRACT_ASSIGNMENT", "1");

    let mut attempt = 0;

    loop {
        match harness.setup_services::<N>(exit_after_registration).await {
            Ok(result) => return Ok(result),
            Err(err) => {
                let message = err.to_string();
                if message.contains(USURPED_NONCE_MSG) {
                    attempt += 1;
                    let jitter: u64 = rand::thread_rng().gen_range(0..250);
                    let backoff_ms = 500 * (attempt as u64).saturating_pow(2) + jitter;
                    warn!(
                        "setup_services attempt {} hit nonce race; retrying in {}ms",
                        attempt, backoff_ms
                    );
                    sleep(Duration::from_millis(backoff_ms)).await;
                    continue;
                }

                return Err(eyre!(err));
            }
        }
    }
}

pub async fn register_jobs_through(
    env: &mut MultiNodeTestEnv<FaaSContext, MockHeartbeatConsumer>,
    max_job_id: u64,
) {
    for job_id in EXECUTE_FUNCTION_JOB_ID..=max_job_id {
        match job_id {
            EXECUTE_FUNCTION_JOB_ID => {
                tracing::info!("Registering job 0 (execute_function)");
                env.add_job(execute_function_job.layer(TangleLayer)).await;
            }
            EXECUTE_ADVANCED_JOB_ID => {
                tracing::info!("Registering job 1 (execute_advanced)");
                env.add_job(execute_advanced_job.layer(TangleLayer)).await;
            }
            CREATE_SNAPSHOT_JOB_ID => {
                tracing::info!("Registering job 2 (create_snapshot)");
                env.add_job(create_snapshot_job.layer(TangleLayer)).await;
            }
            RESTORE_SNAPSHOT_JOB_ID => {
                tracing::info!("Registering job 3 (restore_snapshot)");
                env.add_job(restore_snapshot_job.layer(TangleLayer)).await;
            }
            CREATE_BRANCH_JOB_ID => {
                tracing::info!("Registering job 4 (create_branch)");
                env.add_job(create_branch_job.layer(TangleLayer)).await;
            }
            MERGE_BRANCHES_JOB_ID => {
                tracing::info!("Registering job 5 (merge_branches)");
                env.add_job(merge_branches_job.layer(TangleLayer)).await;
            }
            START_INSTANCE_JOB_ID => {
                tracing::info!("Registering job 6 (start_instance)");
                env.add_job(start_instance_job.layer(TangleLayer)).await;
            }
            STOP_INSTANCE_JOB_ID => {
                tracing::info!("Registering job 7 (stop_instance)");
                env.add_job(stop_instance_job.layer(TangleLayer)).await;
            }
            PAUSE_INSTANCE_JOB_ID => {
                tracing::info!("Registering job 8 (pause_instance)");
                env.add_job(pause_instance_job.layer(TangleLayer)).await;
            }
            RESUME_INSTANCE_JOB_ID => {
                tracing::info!("Registering job 9 (resume_instance)");
                env.add_job(resume_instance_job.layer(TangleLayer)).await;
            }
            EXPOSE_PORT_JOB_ID => {
                tracing::info!("Registering job 10 (expose_port)");
                env.add_job(expose_port_job.layer(TangleLayer)).await;
            }
            UPLOAD_FILES_JOB_ID => {
                tracing::info!("Registering job 11 (upload_files)");
                env.add_job(upload_files_job.layer(TangleLayer)).await;
            }
            _ => tracing::warn!("No registration handler for job id {}", job_id),
        }
    }
}
