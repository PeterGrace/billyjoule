use async_stream::try_stream;
use chrono::{Duration, Utc};
use serenity::futures::{Stream, TryStreamExt};
use serenity::http::Http;
use serenity::model::channel::Message;
use serenity::model::id::{ChannelId, MessageId};
use std::future;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use tracing::{debug, error, info};

struct Sweeper {
    http: Arc<Http>,
    channel_id: ChannelId,
    max_message_age: Duration,
    dry_run: bool,
}

impl Sweeper {
    pub(crate) fn new(
        http: Arc<Http>,
        channel_id: ChannelId,
        max_message_age: Duration,
        dry_run: bool,
    ) -> Self {
        Sweeper {
            http,
            channel_id,
            max_message_age,
            dry_run,
        }
    }

    pub(crate) async fn sweep_messages(&self) -> serenity::Result<u32> {
        let cutoff_time = Utc::now() - self.max_message_age;

        info!(%cutoff_time, "Sweeping expired messages.");
        self.message_stream()
            .try_take_while(|message| future::ready(Ok(message.timestamp.deref() < &cutoff_time)))
            .and_then(|message| async move {
                debug!(self.dry_run, %message.id,  "Found expired message.");

                if self.dry_run {
                    return Ok(0);
                }

                if !self.dry_run {
                    self.channel_id
                        .delete_message(&self.http, message.id)
                        .await
                        .map(|_| 1)
                } else {
                    Ok(0)
                }
            })
            .inspect_err(|error| error!(%error, "Failed to sweep messages."))
            .try_fold(0, |acc, k| future::ready(Ok(acc + k)))
            .await
    }

    fn message_stream(&self) -> Pin<Box<impl Stream<Item = serenity::Result<Message>> + '_>> {
        Box::pin(try_stream! {
            let mut cursor = MessageId(0);
            while let messages = self.load_messages(cursor).await? {
                if messages.is_empty() {
                    break;
                }

                for message in messages.into_iter() {
                    cursor = message.id;
                    yield message;
                }
            }
        })
    }

    async fn load_messages(&self, cursor: impl Into<MessageId>) -> serenity::Result<Vec<Message>> {
        let mut messages = self
            .channel_id
            .messages(&self.http, |b| b.after(cursor.into()).limit(1))
            .await?;

        messages.sort_by_key(|m| m.timestamp);
        Ok(messages)
    }
}
