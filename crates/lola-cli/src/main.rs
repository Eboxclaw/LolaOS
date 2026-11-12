//! `lola` — LOLA OS command-line interface.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use lola_harness::config::{RiskConfig, ToolsConfig};
use lola_infer::config::ModelsConfig;
use lola_infer::residency;
use lola_memory::config::MemoryConfig;
use lola_memory::persistence;
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
    let tools = ToolsConfig::parse(&read_config(config_dir, "tools.yaml")?)?;
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
        "  registry    : {} loadable / {} registered",
        models.models.len(),
        models.registered.len()
    );

    // Every profile declares a model pair; plan residency under its budget.
    for (name, profile) in &memory.profiles {
        let plan = residency::plan(profile.ram_budget_mb, &profile.pair, &models)?;
        println!("  profile [{name}] budget {} MB", profile.ram_budget_mb);
        for (label, slot) in [
            ("slot A (decision)", &plan.decision),
            ("slot B (agent)   ", &plan.agent),
        ] {
            let entry = models.find(&slot.model_id);
            let quant = entry.map_or("?", |e| e.quantization.as_str());
            println!(
                "    {label}: {:<13} {:<6} {:<7} ~{} MB — {}",
                slot.model_id,
                format!("{:?}", slot.modality).to_lowercase(),
                quant,
                slot.est_ram_mb,
                slot.plan.name()
            );
        }
        let mmproj: Vec<&str> = [&plan.decision, &plan.agent]
            .iter()
            .filter_map(|slot| models.find(&slot.model_id)?.mmproj.as_deref())
            .collect();
        if !mmproj.is_empty() {
            println!("    mmproj sidecars: {}", mmproj.join(", "));
        }
        println!(
            "    residency     : {}",
            if plan.fully_resident {
                "fully resident"
            } else {
                "agent swaps on demand (decision never downgraded)"
            }
        );
    }
    println!(
        "  active      : {} — {}",
        memory.active_profile,
        memory.profile()?.description
    );
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
    println!("  compute     : tier-1 only — Rust CPU (AVX2/512·NEON) + Vulkan/wgpu; Adreno probes → CPU fallback");

    // Categorized native tool registry (ADR-033/034).
    let enabled = tools.enabled_categories();
    println!(
        "  tools       : {} categories ({} enabled) · {} tools · read-only default + confirm-on-write",
        tools.categories.len(),
        enabled.len(),
        tools.tool_count()
    );
    for category in &enabled {
        println!(
            "    {:<14} {} tools [{}]",
            category.category.id(),
            category.tools.len(),
            category
                .safety_override
                .map(|s| format!("{s:?}"))
                .unwrap_or_else(|| "default safety".into())
        );
    }
    if let Some(codec) = models.codecs.first() {
        println!(
            "  codec       : {} @ {} (~{} MB) — K9 voice path",
            codec.codec_id, codec.default_quantization, codec.est_ram_mb
        );
    }

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
