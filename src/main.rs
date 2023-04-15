#[macro_use]
extern crate log;

mod models;

use std::env;
use serenity::framework::standard::StandardFramework;
use serenity::prelude::*;
use models::handler::Handler;
use models::handler::GENERAL_GROUP;

#[tokio::main]
async fn main() {

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
