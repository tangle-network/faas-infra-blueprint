use blueprint_sdk::{
    contexts::tangle::TangleClientContext,
    environments::BlueprintEnvironment,
    router::Router,
    runner::BlueprintRunner,
    tangle::{
        config::TangleConfig, consumer::TangleConsumer, layer::TangleLayer,
        producer::TangleProducer,
    },
};
use color_eyre::eyre;
use faas_lib::api_server::{ApiBackgroundService, ApiServerConfig, ApiKeyPermissions};
use faas_lib::context::FaaSContext;
use faas_lib::jobs::*; // Import all jobs
use std::collections::HashMap;
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

    // Build the router with all jobs
    let router = Router::new()
        // Basic execution
        .route(EXECUTE_FUNCTION_JOB_ID, execute_function_job.layer(TangleLayer))
        // Advanced execution with modes
        .route(EXECUTE_ADVANCED_JOB_ID, execute_advanced_job.layer(TangleLayer))
        // Snapshot management
        .route(CREATE_SNAPSHOT_JOB_ID, create_snapshot_job.layer(TangleLayer))
        .route(RESTORE_SNAPSHOT_JOB_ID, restore_snapshot_job.layer(TangleLayer))
        // Branching
        .route(CREATE_BRANCH_JOB_ID, create_branch_job.layer(TangleLayer))
        .route(MERGE_BRANCHES_JOB_ID, merge_branches_job.layer(TangleLayer))
        // Instance management
        .route(START_INSTANCE_JOB_ID, start_instance_job.layer(TangleLayer))
        .route(STOP_INSTANCE_JOB_ID, stop_instance_job.layer(TangleLayer))
        .route(PAUSE_INSTANCE_JOB_ID, pause_instance_job.layer(TangleLayer))
        .route(RESUME_INSTANCE_JOB_ID, resume_instance_job.layer(TangleLayer))
        // Port management
        .route(EXPOSE_PORT_JOB_ID, expose_port_job.layer(TangleLayer))
        // File operations
        .route(UPLOAD_FILES_JOB_ID, upload_files_job.layer(TangleLayer))
        .with_context(context); // Pass the initialized FaaSContext

    // Configure API server
    let mut api_keys = HashMap::new();

    // Add default API key from environment or use a development key
    let api_key = std::env::var("FAAS_API_KEY").unwrap_or_else(|_| "dev-api-key".to_string());
    api_keys.insert(
        api_key.clone(),
        ApiKeyPermissions {
            name: "default".to_string(),
            can_execute: true,
            can_manage_instances: true,
            rate_limit: Some(60), // 60 requests per minute
        },
    );

    let api_config = ApiServerConfig {
        host: std::env::var("FAAS_API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
        port: std::env::var("FAAS_API_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080),
        api_keys,
    };

    info!("API server will listen on {}:{}", api_config.host, api_config.port);
    info!("API key configured: {}", if api_key == "dev-api-key" { "dev-api-key (development)" } else { "custom key from FAAS_API_KEY" });

    // Create background service for API server
    let api_service = ApiBackgroundService::new(api_config, context.clone());

    // Build and run the Blueprint with API server as background service
    info!("Starting BlueprintRunner with API server...");
    BlueprintRunner::builder(TangleConfig::new(), env)
        .router(router)
        .producer(producer)
        .consumer(consumer)
        .background_service(Box::pin(async move {
            if let Err(e) = api_service.run().await {
                tracing::error!("API server error: {}", e);
            }
        }))
        .run()
        .await?;

    Ok(())
}
