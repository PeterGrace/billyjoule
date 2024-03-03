use serenity::model::application::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::prelude::interaction::InteractionResponseType;
use serenity::prelude::*;

pub mod emoji;
pub mod llama;
pub mod stats;

pub async fn err_response(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    error_message: &str,
) {
    debug!("pre-response-send");
    if let Err(e) = command
        .create_interaction_response(&ctx.http, |resp| {
            resp.kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message
                        .content(format!("**error**: {}", error_message))
                        .ephemeral(true)
                })
        })
        .await
    {
        error!("Couldn't send err_response: {}", e);
    }
    debug!("post-response-send");
}
