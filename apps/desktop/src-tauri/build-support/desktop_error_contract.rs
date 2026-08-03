use std::collections::HashSet;

use serde::{Deserialize, Deserializer, de::Error as _};

const SUPPORTED_SCHEMA_VERSION: u32 = 1;
const MAX_MACHINE_CODE_LENGTH: usize = 64;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopCommandErrorContract {
    pub schema_version: u32,
    pub command_errors: Vec<CommandErrorSpec>,
    pub suggested_actions: Vec<SuggestedActionSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandErrorSpec {
    pub code: String,
    pub rust_variant: String,
    pub message_key: String,
    pub severity: String,
    pub actions: Vec<String>,
    #[serde(default)]
    pub reason_codes: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_true_when_present")]
    pub recovery_bundle_path: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SuggestedActionSpec {
    pub code: String,
    pub message_key: String,
}

fn deserialize_true_when_present<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    if bool::deserialize(deserializer)? {
        Ok(true)
    } else {
        Err(D::Error::custom(
            "recoveryBundlePath must be true when present",
        ))
    }
}

pub fn parse_contract(source: &str) -> Result<DesktopCommandErrorContract, String> {
    let contract: DesktopCommandErrorContract = serde_json::from_str(source)
        .map_err(|error| format!("invalid desktop command error contract JSON: {error}"))?;
    validate_contract(&contract)?;
    Ok(contract)
}

pub fn validate_contract(contract: &DesktopCommandErrorContract) -> Result<(), String> {
    if contract.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(format!(
            "unsupported desktop command error contract schema version: {}",
            contract.schema_version
        ));
    }

    validate_unique_codes(
        "suggested action",
        contract
            .suggested_actions
            .iter()
            .map(|action| action.code.as_str()),
    )?;
    let action_codes = contract
        .suggested_actions
        .iter()
        .map(|action| action.code.as_str())
        .collect::<HashSet<_>>();
    for action in &contract.suggested_actions {
        validate_machine_code(&action.code, "suggested action")?;
        validate_message_key(&action.message_key, &action.code)?;
    }

    validate_unique_codes(
        "command error",
        contract
            .command_errors
            .iter()
            .map(|error| error.code.as_str()),
    )?;
    let mut variants = HashSet::new();
    for error in &contract.command_errors {
        validate_machine_code(&error.code, "command error")?;
        validate_rust_variant(&error.rust_variant, &error.code)?;
        if !variants.insert(error.rust_variant.as_str()) {
            return Err(format!(
                "duplicate command error Rust variant: {}",
                error.rust_variant
            ));
        }
        validate_message_key(&error.message_key, &error.code)?;
        if error.severity != "warning" && error.severity != "error" {
            return Err(format!(
                "command error {} has invalid severity: {}",
                error.code, error.severity
            ));
        }
        if error.recovery_bundle_path && error.severity != "error" {
            return Err(format!(
                "command error {} exposes a recovery path but is not an error",
                error.code
            ));
        }
        validate_unique_codes(
            &format!("suggested action on command error {}", error.code),
            error.actions.iter().map(String::as_str),
        )?;
        for action in &error.actions {
            if !action_codes.contains(action.as_str()) {
                return Err(format!(
                    "command error {} references unknown suggested action: {action}",
                    error.code
                ));
            }
        }
        validate_unique_codes(
            &format!("reason code on command error {}", error.code),
            error.reason_codes.iter().map(String::as_str),
        )?;
        for reason in &error.reason_codes {
            validate_machine_code(reason, "reason code")?;
        }
    }

    Ok(())
}

pub fn render_command_error_kinds(contract: &DesktopCommandErrorContract) -> String {
    let variants = contract
        .command_errors
        .iter()
        .map(|error| format!("    {},", error.rust_variant))
        .collect::<Vec<_>>()
        .join("\n");
    let all = contract
        .command_errors
        .iter()
        .map(|error| format!("        Self::{},", error.rust_variant))
        .collect::<Vec<_>>()
        .join("\n");
    let mappings = contract
        .command_errors
        .iter()
        .map(|error| {
            format!(
                "            Self::{} => {:?},",
                error.rust_variant, error.code
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let severity_mappings = contract
        .command_errors
        .iter()
        .map(|error| {
            let severity = if error.severity == "warning" {
                "Warning"
            } else {
                "Error"
            };
            format!(
                "            Self::{} => CommandErrorSeverity::{severity},",
                error.rust_variant
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let reason_mappings = contract
        .command_errors
        .iter()
        .flat_map(|error| {
            error
                .reason_codes
                .iter()
                .map(|reason| format!("            (Self::{}, {:?})", error.rust_variant, reason))
        })
        .collect::<Vec<_>>()
        .join("\n                | ");
    let reason_check = if reason_mappings.is_empty() {
        "false".to_owned()
    } else {
        format!("matches!((self, reason_code),\n{reason_mappings}\n        )")
    };
    let recovery_variants = contract
        .command_errors
        .iter()
        .filter(|error| error.recovery_bundle_path)
        .map(|error| format!("            Self::{}", error.rust_variant))
        .collect::<Vec<_>>()
        .join("\n                | ");
    let recovery_check = if recovery_variants.is_empty() {
        "false".to_owned()
    } else {
        format!("matches!(self,\n{recovery_variants}\n        )")
    };

    format!(
        "// Generated from data/contracts/desktop-command-errors.json. Do not edit.\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
         pub(crate) enum CommandErrorSeverity {{\n    Warning,\n    Error,\n}}\n\n\
         #[allow(dead_code)] // Platform-specific variants are generated for every target.\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n\
         pub(crate) enum CommandErrorKind {{\n{variants}\n}}\n\n\
         impl CommandErrorKind {{\n\
             #[cfg(test)]\n\
             pub(crate) const ALL: &'static [Self] = &[\n{all}\n    ];\n\n\
             #[must_use]\n\
             pub(crate) const fn code(self) -> &'static str {{\n\
                 match self {{\n{mappings}\n        }}\n    }}\n\n\
             #[must_use]\n\
             pub(crate) const fn severity(self) -> CommandErrorSeverity {{\n\
                 match self {{\n{severity_mappings}\n        }}\n    }}\n\n\
             #[must_use]\n\
             pub(crate) fn allows_reason_code(self, reason_code: &str) -> bool {{\n\
                 {reason_check}\n    }}\n\n\
             #[must_use]\n\
             pub(crate) const fn allows_recovery_bundle_path(self) -> bool {{\n\
                 {recovery_check}\n    }}\n}}\n"
    )
}

fn validate_unique_codes<'a>(
    context: &str,
    values: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    let mut seen = HashSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(format!("duplicate {context}: {value}"));
        }
    }
    Ok(())
}

fn validate_machine_code(value: &str, context: &str) -> Result<(), String> {
    let mut chars = value.chars();
    if value.len() > MAX_MACHINE_CODE_LENGTH
        || !matches!(chars.next(), Some('a'..='z'))
        || !chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        return Err(format!("invalid {context}: {value:?}"));
    }
    Ok(())
}

fn validate_rust_variant(value: &str, code: &str) -> Result<(), String> {
    let mut chars = value.chars();
    if !matches!(chars.next(), Some('A'..='Z'))
        || !chars.all(|character| character.is_ascii_alphanumeric())
    {
        return Err(format!(
            "command error {code} has invalid Rust variant: {value:?}"
        ));
    }
    Ok(())
}

fn validate_message_key(value: &str, code: &str) -> Result<(), String> {
    if value.is_empty()
        || value.split('.').any(str::is_empty)
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '.'
        })
    {
        return Err(format!(
            "contract entry {code} has invalid message key: {value:?}"
        ));
    }
    Ok(())
}
