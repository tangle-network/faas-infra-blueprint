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
use faas_lib::context::FaaSContext; // Import the context from faas-lib
use faas_lib::jobs::{EXECUTE_FUNCTION_JOB_ID, execute_function_job};
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

    // Build the router
    let router = Router::new()
        .route(
            EXECUTE_FUNCTION_JOB_ID,
            execute_function_job.layer(TangleLayer), // Apply TangleLayer
        )
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
