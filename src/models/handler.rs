use serenity::async_trait;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::CommandResult;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use tokio::sync::{mpsc, watch};
use tracing::info;

use crate::models::sweeper::Stats;

// event handler
pub struct Handler {
    ready: mpsc::Sender<()>,
    stats: watch::Receiver<Stats>,
}

impl Handler {
    pub(crate) fn new(stats: watch::Receiver<Stats>) -> (Self, mpsc::Receiver<()>) {
        // Using `mpsc` over `oneshot` so we can send a signal without a &mut self.
        let (ready, rx) = mpsc::channel(1);
        (Handler { ready, stats }, rx)
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        self.ready
            .send(())
            .await
            .expect("failed to send start signal");
    }
}

// command groups

#[group]
#[commands(ping)]
struct General;

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, "Pong!").await?;
    Ok(())
}
