use crate::models::sweeper::{Stats, StatsReceiver};
use chrono::Utc;
use human_duration::human_duration;
use serenity::model::application::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::prelude::interaction::InteractionResponseType;
use serenity::prelude::*;

pub async fn do_stats(ctx: &Context, command: ApplicationCommandInteraction) {
    let stats = match get_stats(ctx).await {
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
                        .ephemeral(true)
                })
        })
        .await
    {
        error!(error = %error, "Failed to respond to status command.");
    }
}
async fn get_stats(ctx: &Context) -> Option<Stats> {
    ctx.data
        .read()
        .await
        .get::<StatsReceiver>()
        .map(|sr| sr.borrow().clone())
}
