use std::borrow::BorrowMut;
#[macro_use]
extern crate tracing;

use crate::commands::emoji::do_emoji_indexing;
use crate::models::sweeper::{run_sweeper, Stats, StatsReceiver, Sweeper};
use chrono::Duration;
use clap::Parser;
use duration_string::DurationString;
use models::handler::Handler;
use models::handler::GENERAL_GROUP;
use serenity::framework::standard::StandardFramework;
use serenity::http::Http;
use serenity::model::id::ChannelId;
use serenity::prelude::*;
use std::env;
use std::sync::Arc;
use tokio::sync::{watch, OnceCell};
use tokio::time::{timeout, Timeout};

mod commands;
mod models;

lazy_static::lazy_static! {
    pub static ref CONNECTED: OnceCell<bool> = OnceCell::new();
}

#[derive(Debug, Parser)]
#[command(name = "billyjoule")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Args {
    #[arg(long, env = "GUILD_ID")]
    guild_id: u64,

    #[arg(long, env = "CHANNEL_ID")]
    channel_id: u64,

    #[arg(
        long,
        help = "The age of a message before it's deleted",
        default_value = "1d",
        value_parser = parse_duration,
    )]
    max_message_age: Duration,

    #[arg(
        long,
        help = "When set, does not actually delete messages",
        default_value = "false"
    )]
    dry_run: bool,
}

fn parse_duration(arg: &str) -> Result<Duration, String> {
    arg.parse::<DurationString>()
        .and_then(|ds| Duration::from_std(ds.into()).map_err(|err| err.to_string()))
}

#[tokio::main]
async fn main() {
    // setup logging
    let _ = dotenv::from_path("./billyjoule.env");
    tracing_subscriber::fmt::init();

    // get token
    let token =
        env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set in environment to execute");

    let log_channel_id = env::var("LOG_CHANNEL_ID").ok();

    let meili_server = env::var("MEILISEARCH_URL").ok();

    if let Some(url) = meili_server {
        info!("reindexing emoji folder");
        if let Err(e) = do_emoji_indexing(url).await {
            error!("failure to index emoji: {e}");
        }
    }

    info!(
        "Initializing v:{}, hash:{}",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH")
    );
    // Init sweeper.
    let args = Args::parse();
    let http1 = Http::new(&token);
    let http2 = Http::new(&token);
    let mut stats: Vec<watch::Receiver<Stats>> = Vec::new();

    let (sweeper1, stats1) = Sweeper::new(
        http1,
        args.guild_id.into(),
        args.channel_id.into(),
        args.max_message_age,
        args.dry_run,
    );
    stats.push(stats1);

    // Start sweeper.

    let (sweeper2, stats2) = Sweeper::new(
        http2,
        args.guild_id.into(),
        ChannelId(1391119117154517052),
        Duration::hours(12),
        //args.max_message_age,
        args.dry_run,
    );
    stats.push(stats2);

    // Init handler.
    let handler = Handler::new(args.guild_id.into(), log_channel_id);

    tokio::spawn(run_sweeper(sweeper2, false));
    tokio::spawn(run_sweeper(sweeper1, false));

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let framework = StandardFramework::new()
        .configure(|c| c.prefix("."))
        .group(&GENERAL_GROUP);

    let mut client;
    match timeout(
        core::time::Duration::from_secs(10),
        serenity::Client::builder(&token, intents)
            .framework(framework)
            .event_handler(handler),
    )
    .await
    {
        Ok(Ok(c)) => {
            client = c;
            info!("Connected to Discord");
        }
        Ok(Err(e)) => {
            error!("Failed to connect to Discord: {:?}", e);
            return;
        }
        Err(e) => {
            error!("Failed to connect to Discord: {:?}", e);
            return;
        }
    }

    let mut data = client.data.write().await;
    data.insert::<StatsReceiver>(stats);
    drop(data);

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}
