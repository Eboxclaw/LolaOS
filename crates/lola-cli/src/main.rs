//! `lola` — LOLA OS command-line interface.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use lola_core::config::{MemoryConfig, RiskConfig};
use lola_core::persistence;
use lola_infer::config::ModelsConfig;
use lola_infer::residency;
use lola_venue::config::VenueConfig;

#[derive(Parser)]
#[command(
    name = "lola",
    version,
    about = "LOLA OS — decision-first edge trading harness"
)]
struct Cli {
    #[arg(long, global = true, default_value = "config")]
    config_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate configuration and migrations against a scratch database.
    Doctor,
    /// Apply migrations to the operational database.
    Migrate {
        #[arg(long, default_value = "data/lola.db")]
        db: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Doctor => doctor(&cli.config_dir),
        Command::Migrate { db } => migrate(&db),
    }
}

fn read_config(dir: &Path, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    Ok(std::fs::read_to_string(dir.join(name))?)
}

fn doctor(config_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let models = ModelsConfig::parse(&read_config(config_dir, "models.yaml")?)?;
    let memory = MemoryConfig::parse(&read_config(config_dir, "memory.yaml")?)?;
    let risk = RiskConfig::parse(&read_config(config_dir, "risk.yaml")?)?;
    let venue: VenueConfig = serde_yaml::from_str(&read_config(config_dir, "venue.yaml")?)?;

    println!("LOLA doctor");
    println!(
        "  venue       : {} ({}) — {}",
        venue.venue,
        venue.network,
        venue.products.join("+")
    );
    println!("  watchlist   : {}", venue.watchlist.join(", "));
    println!(
        "  credentials : {} (key management is a carved-out discussion)",
        venue.credentials.mode
    );
    println!(
        "  models      : {} active / {} registered",
        models.active.len(),
        models.registered.len()
    );
    for entry in &models.active {
        println!(
            "    - {:<12} [{:<8}] {} @ {} (~{} MB)",
            entry.model_id, entry.role, entry.repository, entry.quantization, entry.est_ram_mb
        );
    }

    let profile = memory.profile()?;
    println!(
        "  profile     : {} — {}",
        memory.active_profile, profile.description
    );
    let plan = residency::plan(profile.ram_budget_mb, &models);
    for (model_id, slot) in &plan.slots {
        println!("    residency : {:<12} {}", model_id, slot.name());
    }
    println!(
        "  memory ttl  : payloads {}d / sessions {}d / snapshots {}d (immutable: {})",
        memory.ttl.model_payloads_days,
        memory.ttl.session_bulk_days,
        memory.ttl.feature_snapshots_days,
        memory.ttl.immutable.join(", ")
    );
    println!(
        "  risk        : fail_closed={} | testnet max {}x | mainnet {}x→{}x via ladder ({} rungs)",
        risk.fail_closed,
        risk.testnet.max_leverage,
        risk.mainnet.initial_leverage,
        risk.mainnet.max_leverage,
        risk.mainnet.ladder.len()
    );

    let replay = lola_replay::ReplayConfig::default();
    println!(
        "  replay      : next_bar_fills={} fees={}bps spread={}bps funding={}",
        replay.assumptions.next_bar_fills,
        replay.assumptions.taker_fee_bps,
        replay.assumptions.spread_penalty_bps,
        replay.assumptions.funding_accrual
    );

    let scratch = std::env::temp_dir().join(format!("lola-doctor-{}.db", uuid::Uuid::new_v4()));
    let mut conn = persistence::open(&scratch)?;
    let applied = persistence::migrate(&mut conn)?;
    println!("  migrations  : applied {applied:?} on scratch DB — OK");
    drop(conn);
    cleanup(&scratch);

    println!("doctor: all checks passed");
    Ok(())
}

fn migrate(db: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = db.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut conn = persistence::open(db)?;
    let applied = persistence::migrate(&mut conn)?;
    if applied.is_empty() {
        println!("database already current");
    } else {
        println!("applied migrations: {applied:?}");
    }
    Ok(())
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}
