use anyhow::{bail, Result};
use serenity::framework::standard::CommandResult;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::time::Duration;
use tokio::time::sleep;

pub async fn do_exit(ctx: &Context, msg: &Message) -> CommandResult {
    info!("Exiting bot...");
    msg.reply(ctx, "Daisy... Daisy... give me your answer... please...")
        .await?;
    let _ = sleep(Duration::from_secs(1));
    std::process::exit(0);
    Ok(())
}
