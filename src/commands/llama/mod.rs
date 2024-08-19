use anyhow::{bail, Result};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde::Deserialize;
use serde_json::{from_str, json, Value};
use serenity::framework::standard::CommandResult;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::collections::HashMap;
use std::str;

const LLAMA_URL: &str = "http://10.174.5.25:11434";

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
            "model": "wizard-vicuna",
            "prompt": prompt
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
        while let Some(chunk) = response.chunk().await? {
            let pc: ParsedChunk = serde_json::from_slice(chunk.as_ref()).unwrap();
            if let Some(word) = pc.response {
                retval.push(word);
            };
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
            info!(message = s.clone(), "Response:");
            msg.reply(ctx, s.replace(r#"\n"#, "\n")).await?;
        }
        Err(e) => {
            error!(query = query, "failed to execute ollama query.");
            msg.reply(
                ctx,
                "Sorry, I wasn't able to answer your question right now.",
            )
            .await?;
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
