use crate::models::sweeper::{Stats, StatsReceiver};
use chrono::Utc;
use human_duration::human_duration;
use serenity::async_trait;
use serenity::builder::CreateApplicationCommandOption;
use serenity::builder::CreateApplicationCommands;
use serenity::framework::standard::macros::{command, group};
use serenity::framework::standard::CommandResult;
use serenity::model::application::interaction::application_command::{
    ApplicationCommandInteraction, CommandDataOption,
};
use serenity::model::application::interaction::Interaction;
use serenity::model::application::interaction::Interaction::ApplicationCommand;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::id::{ChannelId, GuildId};
use serenity::model::prelude::interaction::InteractionResponseType;
use serenity::prelude::*;
use std::env;

use anyhow::{bail, Result};
use base64::prelude::*;
use s3::creds::Credentials;
use s3::{Bucket, Region};
use serenity::model::prelude::command::{CommandOption, CommandOptionType};
use serenity::utils::MessageBuilder;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

const STATS_COMMAND: &str = "stats";
const STATS_DESCRIPTION: &str = "show stats about the bot";

const EMOJI_COMMAND: &str = "import-emoji";
const EMOJI_DESCRIPTION: &str = "Import emojis";

// event handler
pub struct Handler {
    guild_id: GuildId,
    log_channel_id: Option<String>,
    ready: mpsc::Sender<()>,
}

impl Handler {
    pub(crate) fn new(
        guild_id: GuildId,
        log_channel_id: Option<String>,
    ) -> (Self, mpsc::Receiver<()>) {
        // Using `mpsc` over `oneshot` so we can send a signal without a &mut self.
        let (ready, rx) = mpsc::channel(1);
        (
            Handler {
                guild_id,
                log_channel_id,
                ready,
            },
            rx,
        )
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
            //channel.say(&ctx.http, init_message).await;
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
                            .description(EMOJI_DESCRIPTION)
                            .create_option(|option| {
                                option
                                    .name("emoji")
                                    .kind(CommandOptionType::String)
                                    .required(true)
                                    .description("Name of emoji to import")
                            })
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
            match command.data.name.as_str() {
                STATS_COMMAND => do_stats(&ctx, command).await,
                EMOJI_COMMAND => do_emoji(&ctx, command).await,
                _ => {
                    return;
                }
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

async fn do_stats(ctx: &Context, command: ApplicationCommandInteraction) {
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
                                .field("Version", env!("CARGO_PKG_VERSION"), false)
                                .field("GitHash", env!("GIT_HASH"), false)
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

async fn do_emoji(ctx: &Context, command: ApplicationCommandInteraction) {
    let guild = match command.guild_id {
        Some(g) => g,
        None => {
            error!("No server associated with emoji invocation...?");
            return;
        }
    };
    let emoji_option: Vec<&CommandDataOption> = command
        .data
        .options
        .iter()
        .filter(|opt| opt.name == "emoji")
        .collect();
    let emoji_name = match &emoji_option[0].value {
        Some(s) => s,
        None => {
            error!("Did not receive an emoji name.");
            return;
        }
    };

    let s3_endpoint = env::var("EMOJI_S3_ENDPOINT").ok();
    let s3_bucket = env::var("EMOJI_S3_BUCKET").ok();
    if s3_endpoint.is_none() {
        error!("need an s3 endpoint for emojis");
        return;
    }
    if s3_bucket.is_none() {
        error!("need a bucket name for emojis");
        return;
    }

    let bucket = Bucket::new(
        &s3_bucket.unwrap(),
        Region::Custom {
            region: "us-east-1".to_owned(),
            endpoint: s3_endpoint.unwrap(),
        },
        Credentials::default().unwrap(),
    )
    .unwrap()
    .with_path_style();

    let file_list = match bucket
        .list(
            format!("{}/", emoji_name.as_str().unwrap()),
            Some("/".to_owned()),
        )
        .await
    {
        Ok(s) => s,
        Err(e) => {
            error!("{}", e);
            return;
        }
    };

    let image_data = match bucket.get_object(&file_list[0].contents[0].key).await {
        Ok(rs) => rs,
        Err(e) => {
            error!("Could not retrieve image from s3 bucket");
            return;
        }
    };

    let image_b64 = base64::encode(image_data.as_slice());
    let image_str = format!("data:image/png;base64,{}", image_b64);
    let emoji_name_sanitized = emoji_name.as_str().unwrap().replace("-", "_");
    match guild
        .create_emoji(&ctx.http, &emoji_name_sanitized, &image_str)
        .await
    {
        Ok(t) => {
            if let Err(e) = command
                .create_interaction_response(&ctx.http, |resp| {
                    resp.kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| {
                            message.content(format!(
                                "Emoji :{}: added to server.",
                                &emoji_name_sanitized
                            ))
                        })
                })
                .await
            {
                error!("Unable to send response to command: {}", e);
                return;
            }
        }
        Err(e) => {
            error!("Could not add emoji: {}", e);
            return;
        }
    }
}

// command groups

#[group]
#[commands(ping, version)]
struct General;

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.reply(ctx, "Pong!").await?;
    Ok(())
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
