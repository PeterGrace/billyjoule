#[macro_use]
extern crate tracing;

use std::env;

use chrono::Duration;
use clap::Parser;
use duration_string::DurationString;
use serenity::framework::standard::StandardFramework;
use serenity::http::Http;
use serenity::prelude::*;


use models::handler::Handler;
use models::handler::GENERAL_GROUP;

use crate::models::sweeper::{run_sweeper, StatsReceiver, Sweeper};
use crate::commands::emoji::do_emoji_indexing;

mod models;
mod commands;

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

#[tokio::main(flavor = "current_thread")]
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
        if let Err(_e) = do_emoji_indexing(url).await {
            error!("failure to index emoji");
        }
    }

    info!(
        "Initializing v:{}, hash:{}",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH")
    );
    // Init sweeper.
    let args = Args::parse();
    let http = Http::new(&token);
    let (sweeper, stats) = Sweeper::new(
        http,
        args.guild_id.into(),
        args.channel_id.into(),
        args.max_message_age,
        args.dry_run,
    );

    // Init handler.
    let (handler, ready) = Handler::new(args.guild_id.into(), log_channel_id);

    // Start sweeper.
    tokio::spawn(run_sweeper(sweeper, ready, false));

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let framework = StandardFramework::new()
        .configure(|c| c.prefix("."))
        .group(&GENERAL_GROUP);

    let mut client = serenity::Client::builder(&token, intents)
        .framework(framework)
        .event_handler(handler)
        .await
        .expect("Err creating client");

    let mut data = client.data.write().await;
    data.insert::<StatsReceiver>(stats);
    drop(data);

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}
