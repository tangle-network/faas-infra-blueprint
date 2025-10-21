use blueprint_sdk::{build, tangle::blueprint};
use faas_blueprint_lib::jobs::{
    blueprint_job_definitions::CreateBranchJobDefinition as create_branch_job_metadata,
    blueprint_job_definitions::CreateSnapshotJobDefinition as create_snapshot_job_metadata,
    blueprint_job_definitions::ExecuteAdvancedJobDefinition as execute_advanced_job_metadata,
    blueprint_job_definitions::ExecuteFunctionJobDefinition as execute_function_job_metadata,
    blueprint_job_definitions::ExposePortJobDefinition as expose_port_job_metadata,
    blueprint_job_definitions::MergeBranchesJobDefinition as merge_branches_job_metadata,
    blueprint_job_definitions::PauseInstanceJobDefinition as pause_instance_job_metadata,
    blueprint_job_definitions::RestoreSnapshotJobDefinition as restore_snapshot_job_metadata,
    blueprint_job_definitions::ResumeInstanceJobDefinition as resume_instance_job_metadata,
    blueprint_job_definitions::StartInstanceJobDefinition as start_instance_job_metadata,
    blueprint_job_definitions::StopInstanceJobDefinition as stop_instance_job_metadata,
    blueprint_job_definitions::UploadFilesJobDefinition as upload_files_job_metadata,
};
use std::path::Path;
use std::process;

fn main() {
    // Automatically update dependencies with `soldeer` (if available), and build the contracts.
    //
    // Note that this is provided for convenience, and is not necessary if you wish to handle the
    // contract build step yourself.
    let contracts_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("contracts");

    let contract_dirs: Vec<&str> = vec![contracts_dir.to_str().unwrap()];
    build::soldeer_install();
    build::soldeer_update();
    build::build_contracts(contract_dirs);

    println!("cargo::rerun-if-changed=../faas-blueprint-lib");

    // The `blueprint!` macro generates the info necessary for the `blueprint.json`.
    // See its docs for all available metadata fields.
    let blueprint = blueprint! {
        name: "zk-faas",
        master_manager_revision: "Latest",
        manager: { Evm = "FaaSBlueprint" },
        jobs: [
            execute_function_job_metadata,
            execute_advanced_job_metadata,
            create_snapshot_job_metadata,
            restore_snapshot_job_metadata,
            create_branch_job_metadata,
            merge_branches_job_metadata,
            start_instance_job_metadata,
            stop_instance_job_metadata,
            pause_instance_job_metadata,
            resume_instance_job_metadata,
            expose_port_job_metadata,
            upload_files_job_metadata
        ]
    };

    match blueprint {
        Ok(blueprint) => {
            // TODO: Should be a helper function probably
            let json = blueprint_sdk::tangle::metadata::macros::ext::serde_json::to_string_pretty(
                &blueprint,
            )
            .unwrap();
            std::fs::write(
                Path::new(env!("CARGO_WORKSPACE_DIR")).join("blueprint.json"),
                json.as_bytes(),
            )
            .unwrap();
        }
        Err(e) => {
            println!("cargo::error={e:?}");
            process::exit(1);
        }
    }
}
