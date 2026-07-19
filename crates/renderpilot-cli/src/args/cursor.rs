use std::ffi::OsString;
use std::path::PathBuf;

use crate::CliError;
#[derive(Debug)]
pub(super) struct ArgCursor {
    args: std::vec::IntoIter<OsString>,
}

impl ArgCursor {
    pub(super) fn new(args: impl IntoIterator<Item = OsString>) -> Self {
        Self {
            args: args.into_iter().collect::<Vec<_>>().into_iter(),
        }
    }

    pub(super) fn next_keyword(&mut self) -> Result<Option<String>, CliError> {
        self.args.next().map(parse_os_argument).transpose()
    }

    pub(super) fn next_required_path(
        &mut self,
        argument_name: &'static str,
    ) -> Result<PathBuf, CliError> {
        self.args
            .next()
            .map(PathBuf::from)
            .ok_or(CliError::MissingArgument(argument_name))
    }

    pub(super) fn next_required_keyword(
        &mut self,
        argument_name: &'static str,
    ) -> Result<String, CliError> {
        self.args
            .next()
            .map(parse_os_argument)
            .transpose()?
            .ok_or(CliError::MissingArgument(argument_name))
    }

    pub(super) fn finish(&mut self) -> Result<(), CliError> {
        if let Some(extra) = self.next_keyword()? {
            return Err(CliError::UnexpectedArgument(extra));
        }

        Ok(())
    }
}

fn parse_os_argument(argument: OsString) -> Result<String, CliError> {
    argument
        .into_string()
        .map_err(|_| CliError::NonUnicodeArgument)
}

pub(super) fn parse_named_identifier<T>(
    args: &mut ArgCursor,
    flag: &'static str,
    argument_name: &'static str,
    invalid: fn(String) -> CliError,
) -> Result<T, CliError>
where
    T: TryFrom<String>,
{
    let mut parsed = None;

    while let Some(argument) = args.next_keyword()? {
        if argument != flag {
            return Err(CliError::UnexpectedArgument(argument));
        }

        parsed = Some(parse_identifier_argument(args, argument_name, invalid)?);
    }

    parsed.ok_or(CliError::MissingArgument(argument_name))
}

pub(super) fn parse_identifier_argument<T>(
    args: &mut ArgCursor,
    argument_name: &'static str,
    invalid: fn(String) -> CliError,
) -> Result<T, CliError>
where
    T: TryFrom<String>,
{
    let value = args.next_required_keyword(argument_name)?;

    T::try_from(value.clone()).map_err(|_| invalid(value))
}
