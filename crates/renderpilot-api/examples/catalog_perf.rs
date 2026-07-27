//! Repeatable catalog-query and automatic-scan performance probe.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use renderpilot_api::{QueryGameCardsRequest, bootstrap_games_catalog, query_game_cards};
use renderpilot_orchestration::{Context, catalog};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let database = PathBuf::from(
        args.next()
            .ok_or("usage: catalog_perf <catalog.db> [mode]")?,
    );
    let mode = args.next().unwrap_or_else(|| "queries".to_owned());

    match mode.as_str() {
        "queries" => measure_queries(&database),
        "scan-full" => measure_scan(&database, false),
        "scan-background" => measure_scan(&database, true),
        _ => Err(format!("unknown benchmark mode: {mode}").into()),
    }
}

fn measure_queries(database: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    // Normalize/migrate outside the samples.
    drop(Context::open_at(database)?);

    let open = samples(25, || Context::open_at(database).map(drop))?;
    let context = Context::open_at(database)?;
    let game_count = catalog::list_games(&context)?.len();
    let list_games = samples(200, || catalog::list_games(&context).map(drop))?;
    let request = || QueryGameCardsRequest {
        search_query: String::new(),
        selected_libraries: Vec::new(),
        selected_addons: Vec::new(),
        selected_launchers: Vec::new(),
        launcher_order: Vec::new(),
        show_hidden: false,
        favorites_only: false,
        sort_field: "title".to_owned(),
        sort_direction: "asc".to_owned(),
        page_limit: 5,
        page_offset: 0,
    };

    let cold_query = samples(25, || {
        context.invalidate_catalog_snapshot();
        query_game_cards(&context, request()).map(drop)
    })?;
    let hot_query = samples(500, || query_game_cards(&context, request()).map(drop))?;
    let cold_bootstrap = samples(25, || {
        context.invalidate_catalog_snapshot();
        bootstrap_games_catalog(&context, 120).map(drop)
    })?;
    let hot_bootstrap = samples(200, || bootstrap_games_catalog(&context, 120).map(drop))?;
    let live_validation = samples(5, || {
        renderpilot_api::refresh_validated_catalog_snapshot(&context).map(drop)
    })?;

    println!("catalog_games={game_count}");
    print_stats("context_open", &open);
    print_stats("list_games", &list_games);
    print_stats("query_game_cards_cold", &cold_query);
    print_stats("query_game_cards_hot", &hot_query);
    print_stats("bootstrap_games_catalog_cold", &cold_bootstrap);
    print_stats("bootstrap_games_catalog_hot", &hot_bootstrap);
    print_stats("background_live_validation", &live_validation);
    Ok(())
}

fn measure_scan(database: &PathBuf, background: bool) -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::open_at(database)?;
    let started = Instant::now();
    if background {
        let output = renderpilot_api::scan_auto_libraries_background_output(&context)?;
        println!(
            "scan_changed={} scan_removed={} scan_errors={} changed_ids={:?}",
            output.changed_game_ids.len(),
            output.removed_game_ids.len(),
            output.errors.len(),
            output.changed_game_ids,
        );
    } else {
        renderpilot_api::scan_auto_libraries(&context)?;
    }
    println!("scan_ms={:.3}", started.elapsed().as_secs_f64() * 1_000.0);
    Ok(())
}

fn samples<E>(
    count: usize,
    mut operation: impl FnMut() -> Result<(), E>,
) -> Result<Vec<Duration>, E> {
    let mut durations = Vec::with_capacity(count);
    for _ in 0..count {
        let started = Instant::now();
        operation()?;
        durations.push(started.elapsed());
    }
    Ok(durations)
}

fn print_stats(name: &str, samples: &[Duration]) {
    let mut millis = samples
        .iter()
        .map(|duration| duration.as_secs_f64() * 1_000.0)
        .collect::<Vec<_>>();
    millis.sort_by(f64::total_cmp);
    println!(
        "{name}_ms min={:.3} p50={:.3} p95={:.3} max={:.3}",
        millis[0],
        percentile(&millis, 0.50),
        percentile(&millis, 0.95),
        millis[millis.len() - 1],
    );
}

fn percentile(sorted: &[f64], quantile: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * quantile).ceil() as usize;
    sorted[index]
}
