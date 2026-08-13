pub(in crate::portable_runtime) mod authority;

use std::{ffi::OsString, path::Path};

use semver::Version;

use super::{
    epoch_namespace::establish_epoch,
    error::{PortableRuntimeError, Result},
    generation::{InitialSelectedGeneration, inspect_initial_selection, publish},
    health::validate_selected_app,
    process_admission::AdmissionLock,
    random::hex_32,
    recovery::{recover_prior_transactions, recovery_action},
    rpu::{VerifiedRpu, embedded_rpu, verify_rpu_expected},
    selection::{current_selection, selection_root},
    supervisor_activation::{ActivationContext, CurrentGeneration, activate_generation},
    supervisor_updates::{SupervisorUpdateState, serve_one},
    win32::job::KillOnCloseJob,
};

use self::authority::SupervisorSessionAuthority;

/// Routes the stable raw root before any ordinary desktop initialization.
pub fn dispatch_raw_or_supervisor(args: &[OsString]) -> Result<()> {
    if args.len() != 1 {
        return Ok(());
    }
    run_supervisor()
}

fn run_supervisor() -> Result<()> {
    let raw = std::env::current_exe()?;
    let root = raw
        .parent()
        .ok_or_else(|| PortableRuntimeError::new("portable_root", "raw supervisor had no parent"))?
        .to_owned();
    let authority = SupervisorSessionAuthority::mint(&raw, &root)?;
    if super::win32::directory::directory_identity_digest_no_reparse(&root)?
        != authority.portable_root_identity()
    {
        return Err(PortableRuntimeError::new(
            "portable_supervisor_session",
            "portable root identity changed after supervisor admission",
        ));
    }
    let raw_bytes = std::fs::read(&raw)?;
    let embedded = embedded_rpu(&raw_bytes)?;
    let rpu = verify_rpu_expected(embedded.rpu, embedded.signature, env!("CARGO_PKG_VERSION"))?;
    let generation_store = root.join(".renderpilot-generations").join("v1");
    let update_root = root.join(".renderpilot-update").join("v2");
    let authority_root = root.join(".renderpilot-runtime-authority").join("v1");
    let _admission = AdmissionLock::acquire(&authority_root)?;
    super::provenance::install(&authority_root)?;
    let epoch = hex_32()?;
    let _epoch = establish_epoch(&authority_root, &epoch)?;
    let selection_root = selection_root(&generation_store);
    recover_prior_transactions(
        &generation_store,
        &update_root,
        &root.join("data").join("catalog.db"),
        &selection_root,
        &authority,
    )?;
    let mut current = select_initial_generation(&generation_store, &selection_root, rpu)?;
    let job = KillOnCloseJob::create()?;
    loop {
        // `current` changes after every accepted update. Bind each activation
        // to the generation selected for this iteration, never to the identity
        // captured for the previous App image.
        let generation_identity =
            authority.verify_generation_before_decode(&current.generation_root)?;
        let mut activated = activate_generation(
            ActivationContext {
                root: &root,
                update_root: &update_root,
                selection_root: &selection_root,
                job: &job,
                epoch: &epoch,
                supervisor_session: &authority,
                generation_root_identity: &generation_identity,
            },
            &current,
        )?;
        let mut updates = SupervisorUpdateState::default();
        let staged = loop {
            match serve_one(
                &mut activated.trial,
                &mut updates,
                &current.version,
                &update_root,
            ) {
                Ok(Some(staged)) => break Some(staged),
                Ok(None) => continue,
                Err(_error) if updates.is_uncertain() => {
                    activated.trial.wait_for_exit()?;
                    retain_uncertain_authority()
                }
                Err(error) if error.code() == "portable_runtime_io" => break None,
                Err(error) => return Err(error),
            }
        };
        activated.trial.wait_for_exit()?;
        if let Some(staged) = staged {
            current = publish_next_generation(&generation_store, staged, current)?;
            continue;
        }
        let _ = recovery_action(&activated.journal);
        return Ok(());
    }
}

fn select_initial_generation(
    generation_store: &Path,
    selection_root: &Path,
    rpu: VerifiedRpu,
) -> Result<CurrentGeneration> {
    // Read and validate the entire append-only reducer before publishing any
    // new object. A corrupt/future selection is retained and fails closed.
    let existing_selection = current_selection(selection_root)?;
    let embedded_version = Version::parse(&rpu.manifest.version)
        .map_err(|error| PortableRuntimeError::new("portable_rpu_manifest", error.to_string()))?;

    if let Some(record) = existing_selection {
        match inspect_initial_selection(generation_store, &record.generation_sha256)? {
            InitialSelectedGeneration::Current(stored) => {
                let selected_version = Version::parse(&stored.version).map_err(|error| {
                    PortableRuntimeError::new("portable_generation_receipt", error.to_string())
                })?;
                if selected_version >= embedded_version {
                    return Ok(CurrentGeneration {
                        generation_root: stored.generation_root,
                        app: stored.app,
                        generation_sha256: stored.rpu_sha256,
                        version: stored.version,
                        minimum_supervisor_protocol: stored.minimum_supervisor_protocol,
                        app_session_protocol: stored.app_session_protocol,
                        minimum_schema: stored.minimum_schema,
                        maximum_schema: stored.maximum_schema,
                        selection_predecessor_generation_sha256: Some(record.generation_sha256),
                        quiesced_predecessor_generation_sha256: None,
                    });
                }
            }
            InitialSelectedGeneration::LegacyV2Metadata(legacy) => {
                let selected_version = Version::parse(&legacy.version).map_err(|error| {
                    PortableRuntimeError::new("portable_generation_receipt", error.to_string())
                })?;
                if selected_version >= embedded_version {
                    return Err(PortableRuntimeError::new(
                        "portable_full_package_upgrade_required",
                        "selected legacy portable generation requires a newer full package",
                    ));
                }
            }
        }
        return publish_embedded_generation(generation_store, rpu, Some(record.generation_sha256));
    }

    publish_embedded_generation(generation_store, rpu, None)
}

fn publish_embedded_generation(
    generation_store: &Path,
    rpu: VerifiedRpu,
    selection_predecessor_generation_sha256: Option<String>,
) -> Result<CurrentGeneration> {
    let (generation_root, app) = publish(generation_store, &rpu)?;
    validate_selected_app(&app, &rpu.manifest)?;
    Ok(CurrentGeneration {
        generation_root,
        app,
        generation_sha256: rpu.rpu_sha256,
        version: rpu.manifest.version,
        minimum_supervisor_protocol: rpu.manifest.minimum_supervisor_protocol,
        app_session_protocol: rpu.manifest.app_session_protocol,
        minimum_schema: rpu.manifest.minimum_schema,
        maximum_schema: rpu.manifest.maximum_schema,
        selection_predecessor_generation_sha256,
        quiesced_predecessor_generation_sha256: None,
    })
}

fn publish_next_generation(
    generation_store: &Path,
    staged: VerifiedRpu,
    previous: CurrentGeneration,
) -> Result<CurrentGeneration> {
    let (generation_root, app) = publish(generation_store, &staged)?;
    validate_selected_app(&app, &staged.manifest)?;
    Ok(CurrentGeneration {
        generation_root,
        app,
        generation_sha256: staged.rpu_sha256,
        version: staged.manifest.version,
        minimum_supervisor_protocol: staged.manifest.minimum_supervisor_protocol,
        app_session_protocol: staged.manifest.app_session_protocol,
        minimum_schema: staged.manifest.minimum_schema,
        maximum_schema: staged.manifest.maximum_schema,
        selection_predecessor_generation_sha256: Some(previous.generation_sha256.clone()),
        quiesced_predecessor_generation_sha256: Some(previous.generation_sha256),
    })
}

fn retain_uncertain_authority() -> ! {
    loop {
        std::thread::park_timeout(std::time::Duration::from_secs(60));
    }
}
