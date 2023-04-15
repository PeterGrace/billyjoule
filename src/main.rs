#[macro_use]
extern crate log;

mod models;

use std::env;
use std::time::SystemTime;

use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::framework::standard::{StandardFramework, CommandResult};
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use models::handler::Handler;
use models::handler::GENERAL_GROUP;
use opentelemetry::{global, sdk::export::trace::stdout};
use opentelemetry_sdk::trace::Tracer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{Registry, fmt};

#[tokio::main]
async fn main() {
    //setup tracing
    global::set_text_map_propagator(opentelemetry_jaeger::Propagator::new());
    let tracer = opentelemetry_jaeger::new_agent_pipeline()
        .install_simple()?;
    let opentelemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    tracing_subscriber::registry()
        .with(opentelemetry)
        .with(fmt::Layer::default())
        .try_init()?;


    // setup logging
    if let Err(e) = pretty_env_logger::try_init_timed() {
        eprintln!("logging couldn't initialize: {e}");
        panic!("can't continue when logger offline");
    }

    // get token
    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set in environment to execute");

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let framework = StandardFramework::new()
        .configure(|c| c.prefix("."))
        .group(&GENERAL_GROUP);
    let mut client =
        Client::builder(&token, intents)
            .framework(framework)
            .event_handler(Handler)
            .await
            .expect("Err creating client");
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }

}
