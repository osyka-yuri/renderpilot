pub(in crate::portable_runtime) mod authority;

use std::{ffi::OsString, path::Path};

use semver::Version;

use super::{
    diagnostics_files::{
        PortableDiagnosticSession, open_supervisor, report_diagnostics_failure, report_failure,
    },
    epoch_namespace::establish_epoch,
    error::{PortableRuntimeError, Result},
    generation::{InitialSelectedGeneration, inspect_initial_selection, publish},
    health::{validate_retained_selected_app, validate_selected_app},
    image_authority::{RawSupervisorImage, SelectedGenerationImage},
    process_admission::AdmissionLock,
    random::hex_32,
    recovery::{recover_prior_transactions, recovery_action},
    root_authority::{PortableRootAuthority, SupervisorRootBinding},
    rpu::{VerifiedRpu, embedded_rpu, verify_rpu_expected},
    selection::{SelectionRecord, current_selection, read_selection, selection_root},
    supervisor_activation::{
        ActivationContext, CurrentGeneration, activate_generation_with_diagnostics,
    },
    supervisor_updates::{SupervisorUpdateEvent, SupervisorUpdateState, serve_one},
    win32::job::KillOnCloseJob,
};
use crate::diagnostics::{PortableFailureSite, PortableMilestone};

use self::authority::SupervisorSessionAuthority;

/// Routes the stable raw root before any ordinary desktop initialization.
pub fn dispatch_raw_or_supervisor(args: &[OsString]) -> Result<()> {
    if args.len() != 1 {
        return Ok(());
    }
    run_supervisor()
}

fn run_supervisor() -> Result<()> {
    let raw_path = std::env::current_exe()?;
    let raw_name = raw_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            PortableRuntimeError::new("portable_root", "raw supervisor name was not an ASCII leaf")
        })?;
    let root_path = raw_path
        .parent()
        .ok_or_else(|| PortableRuntimeError::new("portable_root", "raw supervisor had no parent"))?
        .to_owned();
    let generation_store = root_path.join(".renderpilot-generations").join("v1");
    let update_root = root_path.join(".renderpilot-update").join("v2");
    let authority_root = root_path.join(".renderpilot-runtime-authority").join("v1");
    let root = PortableRootAuthority::open(&root_path)?;
    let mut raw_image = RawSupervisorImage::open(&root, raw_name)?;
    let authority = SupervisorSessionAuthority::mint(raw_image.identity(), root.identity())?;
    let binding = SupervisorRootBinding::bind(authority, root)?;
    let _admission = AdmissionLock::acquire(&binding)?;
    let mut diagnostics = match open_supervisor(binding.root().clone(), binding.authority()) {
        Ok(session) => Some(session),
        Err(_) => {
            report_diagnostics_failure();
            None
        }
    };
    let result = run_supervisor_lifecycle(
        &mut raw_image,
        &root_path,
        &binding,
        &generation_store,
        &update_root,
        &authority_root,
        &mut diagnostics,
    );
    if let Some(session) = diagnostics {
        // `_admission` was acquired first and still outlives this explicit
        // close plus exact-handle retention.
        session.close();
    }
    result
}

fn run_supervisor_lifecycle(
    raw_image: &mut RawSupervisorImage,
    root: &Path,
    binding: &SupervisorRootBinding,
    generation_store: &Path,
    update_root: &Path,
    authority_root: &Path,
    diagnostics: &mut Option<PortableDiagnosticSession>,
) -> Result<()> {
    let raw_bytes = observe(
        diagnostics,
        PortableFailureSite::RpuVerify,
        raw_image.rpu_bytes(),
    )?;
    let embedded = observe(
        diagnostics,
        PortableFailureSite::RpuVerify,
        embedded_rpu(&raw_bytes),
    )?;
    let rpu = observe(
        diagnostics,
        PortableFailureSite::RpuVerify,
        verify_rpu_expected(embedded.rpu, embedded.signature, env!("CARGO_PKG_VERSION")),
    )?;
    info(diagnostics, PortableMilestone::RpuVerified);
    observe(
        diagnostics,
        PortableFailureSite::Recovery,
        super::provenance::install(authority_root),
    )?;
    let epoch = observe(diagnostics, PortableFailureSite::Recovery, hex_32())?;
    let _epoch = observe(
        diagnostics,
        PortableFailureSite::Recovery,
        establish_epoch(authority_root, &epoch),
    )?;
    let selection_root = selection_root(generation_store);
    observe(
        diagnostics,
        PortableFailureSite::Recovery,
        recover_prior_transactions(
            generation_store,
            update_root,
            &root.join("data").join("catalog.db"),
            &selection_root,
            binding.authority(),
        ),
    )?;
    info(diagnostics, PortableMilestone::RecoveryComplete);
    let mut current = observe(
        diagnostics,
        PortableFailureSite::GenerationSelect,
        select_initial_generation(generation_store, &selection_root, rpu),
    )?;
    info(diagnostics, PortableMilestone::GenerationSelected);
    let job = observe(
        diagnostics,
        PortableFailureSite::ActivationStart,
        KillOnCloseJob::create(),
    )?;
    loop {
        // `current` changes after every accepted update. Bind each activation
        // to the generation selected for this iteration, never to the identity
        // captured for the previous App image.
        let mut selected_image = observe(
            diagnostics,
            PortableFailureSite::ActivationStart,
            SelectedGenerationImage::open(binding.root(), &current.generation_sha256),
        )?;
        let generation_identity = selected_image.generation_identity().as_str().to_owned();
        observe(
            diagnostics,
            PortableFailureSite::ActivationStart,
            validate_retained_selected_app(selected_image.app_mut(), &current.app_sha256),
        )?;
        let mut activated = activate_generation_with_diagnostics(
            ActivationContext {
                root,
                update_root,
                selection_root: &selection_root,
                job: &job,
                epoch: &epoch,
                supervisor_session: binding.authority(),
                generation_root_identity: &generation_identity,
                portable_root_identity: binding.root().identity().as_str(),
            },
            &current,
            diagnostics.as_mut(),
        )?;
        let mut updates = SupervisorUpdateState::default();
        info(diagnostics, PortableMilestone::UpdateServiceStarted);
        let staged = loop {
            match serve_one(
                &mut activated.trial,
                &mut updates,
                &current.version,
                update_root,
            ) {
                Ok(SupervisorUpdateEvent::ApplyReady(staged)) => break Some(*staged),
                Ok(SupervisorUpdateEvent::Continue) => continue,
                Ok(SupervisorUpdateEvent::AppStatusClosed) => break None,
                Err(_error) if updates.is_uncertain() => {
                    observe(
                        diagnostics,
                        PortableFailureSite::UpdateService,
                        activated.trial.wait_for_exit(),
                    )?;
                    retain_uncertain_authority()
                }
                Err(error) => {
                    report_error(diagnostics, PortableFailureSite::UpdateService, &error);
                    return Err(error);
                }
            }
        };
        observe(
            diagnostics,
            PortableFailureSite::ControlledExit,
            activated.trial.wait_for_successful_exit(),
        )?;
        if let Some(staged) = staged {
            current = observe(
                diagnostics,
                PortableFailureSite::GenerationSelect,
                publish_next_generation(generation_store, staged, current),
            )?;
            info(diagnostics, PortableMilestone::GenerationSelected);
            continue;
        }
        let _ = recovery_action(&activated.journal);
        info(diagnostics, PortableMilestone::ControlledExit);
        return Ok(());
    }
}

fn observe<T>(
    diagnostics: &mut Option<PortableDiagnosticSession>,
    site: PortableFailureSite,
    result: Result<T>,
) -> Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            report_error(diagnostics, site, &error);
            Err(error)
        }
    }
}

fn info(diagnostics: &mut Option<PortableDiagnosticSession>, milestone: PortableMilestone) {
    if let Some(session) = diagnostics.as_mut() {
        let status = session.milestone(milestone);
        super::diagnostics_files::report_emit_failure(status);
    }
}

fn report_error(
    diagnostics: &mut Option<PortableDiagnosticSession>,
    site: PortableFailureSite,
    failure: &PortableRuntimeError,
) {
    report_failure(diagnostics, site, failure);
}

pub(super) fn select_initial_generation(
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
                        app_sha256: stored.app_sha256,
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
            InitialSelectedGeneration::MetadataOnly(predecessor) => {
                require_metadata_only_predecessor_tip(
                    selection_root,
                    &record.generation_sha256,
                    &record.record_sha256,
                )?;
                if predecessor.version >= embedded_version {
                    return Err(PortableRuntimeError::new(
                        "portable_full_package_upgrade_required",
                        "metadata-only predecessor requires a newer full package",
                    ));
                }
                return publish_embedded_generation(
                    generation_store,
                    rpu,
                    Some(record.generation_sha256),
                );
            }
        }
        return publish_embedded_generation(generation_store, rpu, Some(record.generation_sha256));
    }

    publish_embedded_generation(generation_store, rpu, None)
}

fn require_metadata_only_predecessor_tip(
    selection_root: &Path,
    generation_sha256: &str,
    record_sha256: &str,
) -> Result<()> {
    let tip = read_selection(selection_root)?.pop().ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_selection_invalid",
            "released predecessor had no validated selection tip",
        )
    })?;
    if tip.record_sha256 != record_sha256
        || tip.selected_generation_sha256() != Some(generation_sha256)
        || !matches!(tip.record, SelectionRecord::V3(_))
    {
        return Err(PortableRuntimeError::new(
            "portable_selection_invalid",
            "metadata-only predecessor required an exact protocol-3 selection tip",
        ));
    }
    Ok(())
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
        app_sha256: rpu.manifest.app_sha256,
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
        app_sha256: staged.manifest.app_sha256,
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

#[cfg(test)]
mod tests {
    #[test]
    fn supervisor_orders_root_image_binding_admission_diagnostics_and_close() {
        let source = include_str!("supervisor.rs");
        let root = source
            .find("PortableRootAuthority::open")
            .expect("retained root");
        let image = source.find("RawSupervisorImage::open").expect("raw image");
        let mint = source
            .find("SupervisorSessionAuthority::mint")
            .expect("pure session mint");
        let binding = source
            .find("SupervisorRootBinding::bind")
            .expect("root binding");
        let admission = source.find("AdmissionLock::acquire").expect("admission");
        let diagnostics = source
            .find("let mut diagnostics = match open_supervisor")
            .expect("diagnostics");
        let close = source.find("session.close();").expect("diagnostics close");
        assert!(root < image && image < mint && mint < binding && binding < admission);
        assert!(admission < diagnostics && diagnostics < close);
    }
}
