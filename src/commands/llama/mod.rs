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

const LLAMA_URL: &str = "http://dell-r6415.internal:8080/v1";
//const LLAMA_URL: &str = "http://172.17.0.1:9999/v1";

const DISCORD_MSG_SIZE_LIMIT: usize = 2000;
const SYSTEM_PROMPT: &str = r#"
You are a bot running in a discord server full of middle-aged technologists.
They appreciate concise answers when possible.
Don't over-embellish or fluff answers.
Being snarky or witty is definitely appreciated.
Markdown is supported, but prefer using simple paragraph-based text whenever possible.
Try not to use emojis unless it makes sense to do so.
"#;

#[derive(Deserialize)]
struct ParsedChunk {
    model: String,
    created_at: String,
    response: Option<String>,
    done: bool,
}
#[derive(Deserialize)]
struct ChatCompletionResponse {
    model: String,
    created: i64,
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatCompletionMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatCompletionMessage {
    role: String,
    content: String,
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
        let mut messages: Vec<HashMap<String, String>> = vec![];
        messages.push(HashMap::from([
            ("role".to_string(), "system".to_string()),
            ("content".to_string(), SYSTEM_PROMPT.to_string()),
        ]));
        messages.push(HashMap::from([
            ("role".to_string(), "user".to_string()),
            ("content".to_string(), prompt),
        ]));
        let data = json!({
            "model": "Qwen3.6-35B-A3B-Q4",
            "messages": messages,
            "stream": false,
            "max_tokens": 5_000
        });
        info!("Prompt: {messages:#?}");
        let mut response = match self
            .client
            .post(format!("{LLAMA_URL}/chat/completions"))
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

        match response.json::<ChatCompletionResponse>().await {
            Ok(pc) => {
                if pc.choices.len() > 0 {
                    retval.push(pc.choices[0].message.content.clone())
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
