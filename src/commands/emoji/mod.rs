use serenity::json::{json};
use serenity::model::prelude::interaction::InteractionResponseType;
use serenity::prelude::*;
use serenity::model::application::interaction::application_command::{
    ApplicationCommandInteraction, CommandDataOption,
};
use serenity::model::application::interaction::autocomplete::*;
use serde::{Serialize, Deserialize};
use s3::creds::Credentials;
use s3::{Bucket, Region};
use crate::commands::err_response;
use meilisearch_sdk::client::Client as meili;
use meilisearch_sdk::client::*;
use anyhow::Result;

use std::env;

pub async fn do_emoji(ctx: &Context, command: ApplicationCommandInteraction) {
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
        err_response(&ctx, &command, "bot is misconfigured: missing s3 endpoint def").await;
        error!("need an s3 endpoint for emojis");
        return;
    }
    if s3_bucket.is_none() {
        err_response(&ctx, &command, "bot is misconfigured: missing s3 bucket def").await;
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
            err_response(&ctx, &command, "couldn't list s3 contents. maybe wrong bucket or endpoint.").await;
            error!("{}", e);
            return;
        }
    };
    if file_list.len() == 0 {
        err_response(&ctx, &command, "couldn't list s3 contents. maybe wrong bucket or endpoint.").await;
        error!("emoji not found.");
        return;
    }
    if file_list[0].contents.len() == 0 {
        err_response(&ctx, &command, format!("emoji {} not found!",emoji_name).as_str()).await;
        error!("emoji not found.");
        return;
    }

    let image_data = match bucket.get_object(&file_list[0].contents[0].key).await {
        Ok(rs) => rs,
        Err(e) => {
            error!("Could not retrieve image from s3 bucket: {e}");
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
        Ok(_) => {
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

pub async fn do_emoji_autocomplete(ctx: &Context, command: AutocompleteInteraction) {
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
    let choices = json!([
        {"name":"vote","value":"vote"},
        {"name":"dickbutt","value":"dickbutt"}
    ]);
    if let Err(e) = command
        .create_autocomplete_response(&ctx.http, |resp| {
            resp.set_choices(choices)
        }).await {
        error!("couldn't send autocomplete response: {e}");
        return;
    }
}


#[derive(Serialize,Deserialize,Debug)]
pub struct EmojiSearch {
    name: String
}

pub async fn do_emoji_indexing(url: String) -> anyhow::Result<()>{
    let client :meilisearch_sdk::client::Client = meili::new(url, None::<String>);
    client.index("emoji");
    info!("Woulda indexed here.");
    Ok(())
}