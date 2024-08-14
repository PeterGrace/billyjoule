use anyhow::{bail, Result};
use serenity::framework::standard::CommandResult;
use serenity::model::prelude::*;
use serenity::prelude::*;
use yahoo_finance_api as yahoo;

pub async fn do_stonks(ctx: &Context, msg: &Message) -> CommandResult {
    let channel = msg.channel_id;

    let query = msg
        .content
        .clone()
        .strip_prefix(".stonks ")
        .unwrap()
        .to_string();
    let ticker: String;
    match query.split_once(char::is_whitespace) {
        Some((t, o)) => {
            if o.len() > 0 {
                msg.reply(ctx, format!("Please specify one ticker at a time."))
                    .await?;
                return Ok(());
            }
            ticker = t.to_string();
        }
        None => {
            ticker = query;
        }
    }
    let provider = yahoo::YahooConnector::new().unwrap();
    match provider.get_latest_quotes(&ticker, "1d").await {
        Ok(q) => {
            let quote = q.last_quote().unwrap();
            msg.reply(
                ctx,
                format!(
                    "{ticker}: range ${:.2}-${:.2} opened at ${:.2}, latest ${:.2}",
                    quote.low, quote.high, quote.open, quote.adjclose
                ),
            )
            .await?;
        }
        Err(e) => {
            msg.reply(ctx, e).await?;
        }
    };

    Ok(())
}
