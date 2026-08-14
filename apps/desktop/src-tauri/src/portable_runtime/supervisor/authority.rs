//! Ephemeral authority for one portable-supervisor process.
//!
//! The Windows application manifest owns process elevation. This capability
//! only fences one supervisor session and cannot be minted by the managed App.

use serde::{Deserialize, Serialize};

use crate::portable_runtime::{
    error::{PortableRuntimeError, Result},
    random::hex_32,
    signature::sha256_hex,
    win32::object::ObjectIdentity,
};

const SUPERVISOR_SESSION_PROTOCOL: u16 = 3;

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SupervisorSessionRecordV3 {
    protocol: u16,
    session_nonce: String,
    raw_image_identity: String,
    portable_root_identity: String,
}

/// Non-cloneable capability proving that the current raw supervisor validated
/// its executable and portable root before opening durable runtime state.
#[derive(Debug)]
pub(in crate::portable_runtime) struct SupervisorSessionAuthority {
    record: SupervisorSessionRecordV3,
    transcript_sha256: String,
}

impl SupervisorSessionAuthority {
    /// Sole production constructor.  It accepts already-measured retained
    /// capabilities only; this pure protocol role performs no path opens or
    /// identity probes of its own.
    pub(super) fn mint(
        raw_image_identity: &ObjectIdentity,
        portable_root_identity: &ObjectIdentity,
    ) -> Result<Self> {
        let record = SupervisorSessionRecordV3 {
            protocol: SUPERVISOR_SESSION_PROTOCOL,
            session_nonce: hex_32()?,
            raw_image_identity: raw_image_identity.as_str().to_owned(),
            portable_root_identity: portable_root_identity.as_str().to_owned(),
        };
        record.validate()?;
        let transcript_sha256 = record.transcript_sha256()?;
        Ok(Self {
            record,
            transcript_sha256,
        })
    }

    #[cfg(test)]
    pub(in crate::portable_runtime) fn for_test(seed: char) -> Self {
        let digest = seed.to_string().repeat(64);
        let record = SupervisorSessionRecordV3 {
            protocol: SUPERVISOR_SESSION_PROTOCOL,
            session_nonce: digest.clone(),
            raw_image_identity: digest.clone(),
            portable_root_identity: digest,
        };
        let transcript_sha256 = record
            .transcript_sha256()
            .expect("test supervisor session must be valid");
        Self {
            record,
            transcript_sha256,
        }
    }

    #[cfg(test)]
    pub(in crate::portable_runtime) fn for_root_test(root: &ObjectIdentity) -> Self {
        let record = SupervisorSessionRecordV3 {
            protocol: SUPERVISOR_SESSION_PROTOCOL,
            session_nonce: "a".repeat(64),
            raw_image_identity: "b".repeat(64),
            portable_root_identity: root.as_str().to_owned(),
        };
        let transcript_sha256 = record
            .transcript_sha256()
            .expect("test supervisor root binding must be valid");
        Self {
            record,
            transcript_sha256,
        }
    }

    pub(in crate::portable_runtime) fn transcript_sha256(&self) -> &str {
        &self.transcript_sha256
    }

    pub(in crate::portable_runtime) fn portable_root_identity(&self) -> &str {
        &self.record.portable_root_identity
    }
}

impl SupervisorSessionRecordV3 {
    fn validate(&self) -> Result<()> {
        if self.protocol != SUPERVISOR_SESSION_PROTOCOL
            || !is_digest(&self.session_nonce)
            || !is_digest(&self.raw_image_identity)
            || !is_digest(&self.portable_root_identity)
        {
            return Err(PortableRuntimeError::new(
                "portable_supervisor_session",
                "supervisor session record did not satisfy protocol v3",
            ));
        }
        Ok(())
    }

    fn transcript_sha256(&self) -> Result<String> {
        self.validate()?;
        serde_json::to_vec(self)
            .map(|bytes| sha256_hex(&bytes))
            .map_err(|error| {
                PortableRuntimeError::new("portable_supervisor_session", error.to_string())
            })
    }
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
