use anyhow::{bail, Result};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde::Deserialize;
use serde_json::{from_str, json, Value};
use serenity::framework::standard::{CommandError, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::collections::HashMap;
use std::str;

const LLAMA_URL: &str = "http://10.174.5.25:11434";
const DISCORD_MSG_SIZE_LIMIT: usize = 2000;

#[derive(Deserialize)]
struct ParsedChunk {
    model: String,
    created_at: String,
    response: Option<String>,
    done: bool,
}

pub struct OllamaApi {
    client: reqwest_middleware::ClientWithMiddleware,
}

impl OllamaApi {
    pub fn new() -> Self {
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(10);
        let rclient = reqwest_middleware::ClientBuilder::new(reqwest::Client::new())
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();
        OllamaApi { client: rclient }
    }
    pub async fn get_models(&self) -> Result<String> {
        let mut rs = match self
            .client
            .get(format!("{LLAMA_URL}/api/tags"))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => bail!(e),
        };
        Ok(String::from_utf8(Vec::from(rs.bytes().await.unwrap())).unwrap())
    }
    pub async fn doit(&self, prompt: String) -> Result<String> {
        let data = json!({
            "model": "wizard-vicuna-uncensored",
            "prompt": prompt,
            "stream": false
        });
        info!("Prompt: {prompt}");
        let mut response = match self
            .client
            .post(format!("{LLAMA_URL}/api/generate"))
            .json(&data)
            .send()
            .await
        {
            Ok(r) => {
                info!("Received acceptable response to POST request, now entering wait-for-response phase.");
                r
            }
            Err(e) => {
                bail!("Error making a call to the generate endpoint: {e}");
            }
        };
        let mut retval: Vec<String> = vec![];

        match response.json::<ParsedChunk>().await {
            Ok(pc) => {
                if let Some(word) = pc.response {
                    retval.push(word);
                } else {
                    let msg = format!("Empty response from API.");
                    warn!(msg);
                    bail!(msg);
                }
            }
            Err(e) => {
                let msg = format!("Failed to parse response as JSON: {e}");
                warn!(msg);
                bail!(msg);
            }
        }

        Ok(retval.join(""))
    }
}

pub async fn do_llama(ctx: &Context, msg: &Message) -> CommandResult {
    let channel = msg.channel_id;
    info!("Set typing");
    let typing = channel.start_typing(&ctx.http).ok();

    let query = msg
        .content
        .clone()
        .strip_prefix(".llama")
        .unwrap()
        .to_string();

    // confirm for the user we're processing
    msg.reply(ctx, "Give me a moment and I'll fetch you an answer.")
        .await?;
    let ollama = OllamaApi::new();
    let response = match ollama.doit(query.clone()).await {
        Ok(s) => {
            if s.len() > DISCORD_MSG_SIZE_LIMIT {
                let whole_payload = s
                    .split_inclusive("\n")
                    .collect::<Vec<&str>>()
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>();
                let mut paragraphs: Vec<String> = vec![];
                let mut collector: Vec<String> = vec![];
                let mut collector_len = 0;
                for line in whole_payload.iter() {
                    collector.push(line.to_string());
                    collector_len += line.len();
                    if collector_len > DISCORD_MSG_SIZE_LIMIT / 2 {
                        paragraphs.push(collector.join(" "));
                        collector.clear();
                        collector_len = 0;
                    }
                }
                for paragraph in paragraphs.iter() {
                    if let Err(e) = msg.reply(ctx, paragraph.clone()).await {
                        error!(message = paragraph.clone(), "Failed to send response: {e}");
                    }
                }
            } else {
                if let Err(e) = msg.reply(ctx, s.replace(r#"\n"#, "\n")).await {
                    error!(message = s.clone(), "Failed to send response: {e}");
                    if let Err(ee) = msg
                        .reply(
                            ctx,
                            "Sorry, I wasn't able to answer your question right now.",
                        )
                        .await
                    {
                        error!("Failed to send error response to chat: {ee}");
                    }
                }
            }
        }
        Err(e) => {
            error!(query = query, "failed to execute ollama query.");
            if let Err(ee) = msg
                .reply(
                    ctx,
                    "Sorry, I wasn't able to answer your question right now.",
                )
                .await
            {
                error!("Failed to send error response to chat: {ee}");
            };
        }
    };

    if typing.is_some() {
        typing.unwrap().stop();
    }
    Ok(())
}

pub async fn do_llama_models(ctx: &Context, msg: &Message) -> CommandResult {
    let channel = msg.channel_id;
    let ollama = OllamaApi::new();
    let response = match ollama.get_models().await {
        Ok(s) => {
            msg.reply(ctx, s.replace(r#"\n"#, "\n")).await?;
        }
        Err(e) => {
            msg.reply(
                ctx,
                "Sorry, I wasn't able to answer your question right now.",
            )
            .await?;
        }
    };

    Ok(())
}
