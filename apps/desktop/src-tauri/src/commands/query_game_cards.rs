use serde::Deserialize;

use super::{
    CommandBoundary,
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
    fn as_api_value(&self) -> &'static str {
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
    fn as_api_value(&self) -> &'static str {
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
    fn into_api_values(self) -> (String, String) {
        (
            self.field.as_api_value().to_owned(),
            self.direction.as_api_value().to_owned(),
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
    fn into_api_values(self) -> Result<(i64, i64), CommandError> {
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
    selected_addons: Vec<String>,

    #[serde(default)]
    selected_launchers: Vec<String>,

    #[serde(default)]
    launcher_order: Vec<String>,

    #[serde(default)]
    show_hidden: bool,

    #[serde(default)]
    favorites_only: bool,

    sort: GameCardsSortDto,
    page: GameCardsPageDto,
}

pub(crate) struct QueryGameCardsArgs {
    pub(super) search_query: String,
    pub(super) selected_libraries: Vec<String>,
    pub(super) selected_addons: Vec<String>,
    pub(super) selected_launchers: Vec<String>,
    pub(super) launcher_order: Vec<String>,
    pub(super) show_hidden: bool,
    pub(super) favorites_only: bool,
    pub(super) sort_field: String,
    pub(super) sort_direction: String,
    pub(super) limit: i64,
    pub(super) offset: i64,
}

impl QueryGameCardsDto {
    pub(super) fn into_desktop_args(
        self,
        boundary: &CommandBoundary,
    ) -> Result<QueryGameCardsArgs, CommandError> {
        let search_query = trim_string(&self.search_query);
        let selected_libraries = trim_string_vec(self.selected_libraries);
        let selected_addons = trim_string_vec(self.selected_addons);
        let selected_launchers = trim_string_vec(self.selected_launchers);
        let launcher_order = trim_string_vec(self.launcher_order);

        reject_empty_items(boundary, "selected_libraries", &selected_libraries)?;
        reject_empty_items(boundary, "selected_addons", &selected_addons)?;
        reject_empty_items(boundary, "selected_launchers", &selected_launchers)?;
        reject_empty_items(boundary, "launcher_order", &launcher_order)?;

        let (sort_field, sort_direction) = self.sort.into_api_values();
        let (limit, offset) = self
            .page
            .into_api_values()
            .map_err(|error| boundary.record(error))?;

        Ok(QueryGameCardsArgs {
            search_query,
            selected_libraries,
            selected_addons,
            selected_launchers,
            launcher_order,
            show_hidden: self.show_hidden,
            favorites_only: self.favorites_only,
            sort_field,
            sort_direction,
            limit,
            offset,
        })
    }
}
