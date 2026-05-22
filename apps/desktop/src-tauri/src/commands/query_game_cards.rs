use serde::Deserialize;

use super::{
    error::CommandError,
    validation::{reject_empty_items, trim_string, trim_string_vec},
};

const MAX_GAME_CARDS_PAGE_LIMIT: u32 = 10_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum GameCardsSortFieldDto {
    Title,
    Updates,
    Risk,
}

impl GameCardsSortFieldDto {
    fn as_cli_value(&self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Updates => "updates",
            Self::Risk => "risk",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum GameCardsSortDirectionDto {
    Asc,
    Desc,
}

impl GameCardsSortDirectionDto {
    fn as_cli_value(&self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GameCardsSortDto {
    field: GameCardsSortFieldDto,
    direction: GameCardsSortDirectionDto,
}

impl GameCardsSortDto {
    fn into_cli_values(self) -> (String, String) {
        (
            self.field.as_cli_value().to_owned(),
            self.direction.as_cli_value().to_owned(),
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GameCardsPageDto {
    limit: u32,
    offset: u32,
}

impl GameCardsPageDto {
    fn into_cli_values(self) -> Result<(i64, i64), CommandError> {
        if self.limit == 0 {
            return Err(CommandError::invalid_argument(
                "limit",
                "must be greater than 0",
            ));
        }

        if self.limit > MAX_GAME_CARDS_PAGE_LIMIT {
            return Err(CommandError::invalid_argument(
                "limit",
                "must not exceed maximum page size",
            ));
        }

        Ok((i64::from(self.limit), i64::from(self.offset)))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct QueryGameCardsDto {
    #[serde(default)]
    search_query: String,

    #[serde(default)]
    selected_libraries: Vec<String>,

    #[serde(default)]
    selected_launchers: Vec<String>,

    sort: GameCardsSortDto,
    page: GameCardsPageDto,
}

pub(crate) struct QueryGameCardsArgs {
    pub(super) search_query: String,
    pub(super) selected_libraries: Vec<String>,
    pub(super) selected_launchers: Vec<String>,
    pub(super) sort_field: String,
    pub(super) sort_direction: String,
    pub(super) limit: i64,
    pub(super) offset: i64,
}

impl QueryGameCardsDto {
    pub(super) fn into_desktop_args(self) -> Result<QueryGameCardsArgs, CommandError> {
        let search_query = trim_string(self.search_query);
        let selected_libraries = trim_string_vec(self.selected_libraries);
        let selected_launchers = trim_string_vec(self.selected_launchers);

        reject_empty_items("selected_libraries", &selected_libraries)?;
        reject_empty_items("selected_launchers", &selected_launchers)?;

        let (sort_field, sort_direction) = self.sort.into_cli_values();
        let (limit, offset) = self.page.into_cli_values()?;

        Ok(QueryGameCardsArgs {
            search_query,
            selected_libraries,
            selected_launchers,
            sort_field,
            sort_direction,
            limit,
            offset,
        })
    }
}
