use crate::models::sweeper::{Stats, StatsReceiver};
use chrono::Utc;
use human_duration::human_duration;
use serenity::model::application::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::prelude::interaction::InteractionResponseType;
use serenity::prelude::*;

pub async fn do_stats(ctx: &Context, command: ApplicationCommandInteraction) {
    let vec_stats = match get_stats(ctx).await {
        None => {
            error!("Stats don't exist, but they should.");
            return;
        }
        Some(stats) => stats,
    };

    if let Err(error) = command
        .create_interaction_response(&ctx.http, |resp| {
            resp.kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|mut message| {
                    let uptime = (Utc::now() - vec_stats[0].started)
                        .to_std()
                        .expect("Duration should be in range");

                    message
                        .content(":wave: Hey there, here are some sweeper stats")
                        .embed(|embed| {
                            embed
                                .field("Version", env!("CARGO_PKG_VERSION"), false)
                                .field("GitHash", env!("GIT_HASH"), false)
                                .field("Uptime", human_duration(&uptime), false)
                        });

                    for stats in vec_stats {
                        message = message.embed(|embed| {
                            embed
                                .field("Channel", stats.channel_id, false)
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
                    }
                    message = message.ephemeral(true);
                    message
                })
        })
        .await
    {
        error!(error = %error, "Failed to respond to status command.");
    }
}
async fn get_stats(ctx: &Context) -> Option<Vec<Stats>> {
    let stats = ctx
        .data
        .read()
        .await
        .get::<StatsReceiver>()
        .unwrap()
        .iter()
        .map(|sr| sr.borrow().clone())
        .collect();
    Some(stats)
}
