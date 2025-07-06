use crate::commands::emoji::{do_emoji, do_emoji_autocomplete};
use crate::commands::llama::{do_llama, do_llama_models};
use crate::commands::stats::do_stats;
use crate::commands::stonks::do_stonks;
use crate::CONNECTED;
use serenity::async_trait;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::CommandResult;
use serenity::model::application::interaction::Interaction;
use serenity::model::application::interaction::Interaction::{ApplicationCommand, Autocomplete};
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::{ChannelId, GuildId};
use serenity::model::permissions::Permissions;
use serenity::model::prelude::command::CommandOptionType;
use serenity::prelude::*;
use serenity::utils::MessageBuilder;
use std::env;
use tokio::sync::mpsc;

const STATS_COMMAND: &str = "stats";
const STATS_DESCRIPTION: &str = "show stats about the bot";

const EMOJI_COMMAND: &str = "import-emoji";
const EMOJI_DESCRIPTION: &str = "Import emojis";

// event handler
pub struct Handler {
    guild_id: GuildId,
    log_channel_id: Option<String>,
}

impl Handler {
    pub(crate) fn new(guild_id: GuildId, log_channel_id: Option<String>) -> Self {
        Handler {
            guild_id,
            log_channel_id,
        }
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);

        if self.log_channel_id.is_some() {
            let channel = ChannelId(self.log_channel_id.clone().unwrap().parse().unwrap());
            let init_message = MessageBuilder::new()
                .push(format!(
                    "System is ready.   v:{}, hash:{}",
                    env!("CARGO_PKG_VERSION"),
                    env!("GIT_HASH")
                ))
                .build();
            if let Err(e) = channel.say(&ctx.http, init_message).await {
                error!("Couldn't use channel.say in eventhandler: {e}")
            };
        };
        // Configure stats command.
        self.guild_id
            .set_application_commands(&ctx.http, |builder| {
                builder
                    .create_application_command(|command| {
                        command.name(STATS_COMMAND).description(STATS_DESCRIPTION)
                    })
                    .create_application_command(|command| {
                        command
                            .name(EMOJI_COMMAND)
                            .default_member_permissions(Permissions::MANAGE_EMOJIS_AND_STICKERS)
                            .description(EMOJI_DESCRIPTION)
                            .create_option(|option| {
                                option
                                    .name("emoji")
                                    .kind(CommandOptionType::String)
                                    .required(true)
                                    .description("Name of emoji to import")
                                    .set_autocomplete(true)
                            })
                    })
            })
            .await
            .expect("failed to create app commands");
        info!("Created app commands");

        CONNECTED
            .set(true)
            .expect("Couldn't set CONNECTED, nothing will work after this message so paniking");
    }
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let ApplicationCommand(command) = interaction.clone() {
            match command.data.name.as_str() {
                STATS_COMMAND => do_stats(&ctx, command).await,
                EMOJI_COMMAND => do_emoji(&ctx, command).await,
                _ => {
                    return;
                }
            }
        };
        if let Autocomplete(command) = interaction {
            match command.data.name.as_str() {
                EMOJI_COMMAND => do_emoji_autocomplete(&ctx, command).await,
                _ => {
                    return;
                }
            };
        };
    }
}

// command groups

#[group]
#[commands(ping, llama, llama_models, stonks, version)]
struct General;

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, "Pong!").await?;
    Ok(())
}

#[command]
async fn llama_models(ctx: &Context, msg: &Message) -> CommandResult {
    do_llama_models(ctx, msg).await
}

#[command]
async fn llama(ctx: &Context, msg: &Message) -> CommandResult {
    info!("Received llama request.");
    do_llama(ctx, msg).await
}

#[command]
async fn stonks(ctx: &Context, msg: &Message) -> CommandResult {
    info!("Received stonks request.");
    do_stonks(ctx, msg).await
}

#[command]
async fn version(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(
        ctx,
        format!(
            "Running v:`{}`, hash:`{}`",
            env!("CARGO_PKG_VERSION"),
            env!("GIT_HASH")
        ),
    )
    .await?;
    Ok(())
}
