use crate::models::sweeper::{Stats, StatsReceiver};
use chrono::Utc;
use human_duration::human_duration;
use serenity::async_trait;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::CommandResult;
use serenity::model::application::interaction::Interaction;
use serenity::model::application::interaction::Interaction::ApplicationCommand;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::GuildId;
use serenity::model::prelude::interaction::InteractionResponseType;
use serenity::prelude::*;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

// event handler
pub struct Handler {
    guild_id: GuildId,
    ready: mpsc::Sender<()>,
}

impl Handler {
    pub(crate) fn new(guild_id: GuildId) -> (Self, mpsc::Receiver<()>) {
        // Using `mpsc` over `oneshot` so we can send a signal without a &mut self.
        let (ready, rx) = mpsc::channel(1);
        (Handler { guild_id, ready }, rx)
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        // Configure stats command.
        self.guild_id
            .set_application_commands(&ctx.http, |builder| {
                builder.create_application_command(|command| {
                    command.name("stats").description("Get sweeper stats")
                })
            })
            .await
            .expect("failed to create app commands");
        info!("Created app commands");

        self.ready
            .send(())
            .await
            .expect("failed to send start signal");
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let ApplicationCommand(command) = interaction {
            if command.data.name != "stats" {
                debug!(command = command.data.name, "Received unknown command.");
                return;
            }

            let stats = match get_stats(&ctx).await {
                None => {
                    error!("Stats don't exist, but they should.");
                    return;
                }
                Some(stats) => stats,
            };

            if let Err(error) = command
                .create_interaction_response(&ctx.http, |resp| {
                    resp.kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| {
                            let uptime = (Utc::now() - stats.started)
                                .to_std()
                                .expect("Duration should be in range");

                            message
                                .content(":wave: Hey there, here are some sweeper stats")
                                .embed(|embed| {
                                    embed
                                        .field("Uptime", human_duration(&uptime), false)
                                        .field("Runs", format!("Ran {} times", stats.runs), false)
                                        .field(
                                            "Last Run",
                                            format!("Cleaned up {} messages.", stats.last_run),
                                            false,
                                        )
                                        .field(
                                            "All Runs",
                                            format!("Cleaned up {} messages.", stats.all_runs),
                                            false,
                                        )
                                })
                        })
                })
                .await
            {
                error!(error = %error, "Failed to respond to status command.");
            }
        }
    }
}

async fn get_stats(ctx: &Context) -> Option<Stats> {
    ctx.data
        .read()
        .await
        .get::<StatsReceiver>()
        .map(|sr| sr.borrow().clone())
}

// command groups

#[group]
#[commands(ping)]
#[commands(version)]
struct General;

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, "Pong!").await?;
    Ok(())
}
#[command]
async fn version(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, format!("Running v:{}, hash:{}",env!("CARGO_PKG_VERSION"), env!("GIT_HASH"))).await?;
    Ok(())
}
