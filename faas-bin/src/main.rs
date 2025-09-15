use blueprint_sdk::{
    contexts::tangle::TangleClientContext,
    environments::BlueprintEnvironment,
    router::Router,
    runner::BlueprintRunner,
    tangle::{
        consumer::TangleConsumer,
        producer::TangleProducer,
        config::TangleConfig,
        layer::TangleLayer,
    },
};
use color_eyre::eyre;
use faas_lib::context::FaaSContext; // Import the context from faas-lib
use faas_lib::jobs::{execute_function_job, EXECUTE_FUNCTION_JOB_ID};
use tracing::info;

// --- Main Blueprint Setup ---

#[tokio::main]
async fn main() -> eyre::Result<()> {
    dotenvy::dotenv().ok();
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("Starting FaaS Blueprint Service...");

    let env = BlueprintEnvironment::boot().await?;

    // Context creation now handles orchestrator setup
    info!("Initializing FaaSContext...");
    let context = FaaSContext::new(env.clone()).await?;
    info!("FaaSContext initialized.");

    // Standard Tangle setup
    let signer = env.tangle_identities().first_signer().await?;
    let client = env.tangle_chain_client(None).await?;
    let producer = TangleProducer::finalized_blocks(env.tangle_client()).await?;
    let consumer = TangleConsumer::new(client.rpc_client.clone(), signer);

    // Build the router
    let router = Router::new()
        .route(
            EXECUTE_FUNCTION_JOB_ID,
            execute_function_job.layer(TangleLayer), // Apply TangleLayer
        )
        .with_context(context); // Pass the initialized FaaSContext

    // Build and run the Blueprint
    info!("Starting BlueprintRunner...");
    BlueprintRunner::builder(TangleConfig::new(), env)
        .router(router)
        .producer(producer)
        .consumer(consumer)
        .run()
        .await?;

    Ok(())
}
