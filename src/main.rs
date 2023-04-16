mod models;

use models::handler::Handler;
use models::handler::GENERAL_GROUP;
use serenity::framework::standard::StandardFramework;
use serenity::prelude::*;
use std::env;
use tracing::error;

#[tokio::main]
async fn main() {
    // setup logging
    tracing_subscriber::fmt::init();

    // get token
    let token =
        env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set in environment to execute");

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let framework = StandardFramework::new()
        .configure(|c| c.prefix("."))
        .group(&GENERAL_GROUP);
    let mut client = Client::builder(&token, intents)
        .framework(framework)
        .event_handler(Handler)
        .await
        .expect("Err creating client");
    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}
