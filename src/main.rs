use std::env;

use chrono::Duration;
use clap::Parser;
use duration_string::DurationString;
use serenity::framework::standard::StandardFramework;
use serenity::http::Http;
use serenity::prelude::*;
use tracing::error;

use models::handler::Handler;
use models::handler::GENERAL_GROUP;

use crate::models::sweeper::{run_sweeper, StatsReceiver, Sweeper};

mod models;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long)]
    guild_id: u64,

    #[arg(long)]
    channel_id: u64,

    #[arg(long, default_value = "1d", value_parser = parse_duration)]
    max_message_age: Duration,

    #[arg(long, default_value = "false")]
    dry_run: bool,
}

fn parse_duration(arg: &str) -> Result<Duration, String> {
    arg.parse::<DurationString>()
        .and_then(|ds| Duration::from_std(ds.into()).map_err(|err| err.to_string()))
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // setup logging
    tracing_subscriber::fmt::init();

    // get token
    let token =
        env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set in environment to execute");

    // Init sweeper.
    let args = Args::parse();
    let http = Http::new(&token);
    let (sweeper, stats) = Sweeper::new(
        http,
        args.channel_id.into(),
        args.max_message_age,
        args.dry_run,
    );

    // Init handler.
    let (handler, ready) = Handler::new(args.guild_id.into());

    // Start sweeper.
    tokio::spawn(run_sweeper(sweeper, ready));

    let intents = GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES;

    let framework = StandardFramework::new()
        .configure(|c| c.prefix("."))
        .group(&GENERAL_GROUP);

    let mut client = Client::builder(&token, intents)
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
