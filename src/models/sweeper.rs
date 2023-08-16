
use async_recursion::async_recursion;
use async_stream::try_stream;
use chrono::{DateTime, Duration, Utc};
use futures::stream::StreamExt;
use serenity::futures::{Stream, TryStreamExt};
use serenity::http::Http;
use serenity::model::channel::Message;
use serenity::model::id::{ChannelId, GuildId, MessageId};
use serenity::prelude::TypeMapKey;
use serenity::utils::MessageBuilder;

use std::ops::Deref;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::{env, future};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use tracing::{debug, error, info};

pub(crate) async fn run_sweeper(mut sweeper: Sweeper, mut ready: mpsc::Receiver<()>, once: bool) {
    ready.recv().await.expect("failed to receive ready signal");

    info!("Bot ready, starting sweep loop.");
    loop {
        let start = Instant::now();
        sweeper.sweep_messages().await;
        info!(
            task_millis = start.elapsed().as_millis(),
            stats = ?sweeper.stats,
            "Ran sweeper. Sleeping for 1 hour."
        );

        if once {
            info!("Instantiated with once = true, exiting 1-hr loop logic.");
            return;
        }
        tokio::time::sleep(Duration::hours(1).to_std().expect("1 hour is in range")).await;
    }
}

pub(crate) struct Sweeper {
    http: Arc<Http>,
    guild_id: GuildId,
    channel_id: ChannelId,
    max_message_age: Duration,
    dry_run: bool,
    log_channel: Option<ChannelId>,
    stats: Stats,
    stats_tx: watch::Sender<Stats>,
}

#[derive(Debug, Clone)]
pub(crate) struct Stats {
    pub(crate) started: DateTime<Utc>,
    pub(crate) runs: u32,
    pub(crate) last_run: u32,
    pub(crate) all_runs: u32,
}

pub(crate) struct StatsReceiver;

impl TypeMapKey for StatsReceiver {
    type Value = watch::Receiver<Stats>;
}

impl Sweeper {
    pub(crate) fn new(
        http: Http,
        guild_id: GuildId,
        channel_id: ChannelId,
        max_message_age: Duration,
        dry_run: bool,
    ) -> (Self, watch::Receiver<Stats>) {
        let stats = Stats {
            started: Utc::now(),
            runs: 0,
            last_run: 0,
            all_runs: 0,
        };

        let (tx, rx) = watch::channel(stats.clone());

        let mut log_channel: Option<ChannelId> = None;
        if let Ok(log_channel_id) = env::var("LOG_CHANNEL_ID") {
            log_channel = match log_channel_id.parse::<u64>() {
                Ok(val) => Some(ChannelId(val)),
                Err(_) => None,
            }
        }

        (
            Sweeper {
                http: Arc::new(http),
                guild_id,
                channel_id,
                log_channel,
                max_message_age,
                dry_run,
                stats,
                stats_tx: tx,
            },
            rx,
        )
    }

    #[async_recursion]
    async fn sweep_messages(&mut self) {
        let cutoff_time = Utc::now() - self.max_message_age;

        let success_count = Arc::new(AtomicU32::new(0));

        if let Ok(threads) = self.guild_id.get_active_threads(self.http.clone()).await {
            for thread in threads.threads {
                if thread.parent_id == Some(self.channel_id) {
                    info!("Thread {} is in scope for sweeping", thread.name);
                    // we already know DISCORD_TOKEN is valid since we expect it in main.rs when starting
                    let token = env::var("DISCORD_TOKEN").unwrap();
                    let (mut sweeper, _stats) = Sweeper::new(
                        Http::new(&token),
                        thread.guild_id,
                        thread.id,
                        Duration::days(1),
                        false,
                    );
                    // Start sweeper on thread.
                    sweeper.sweep_messages().await;
                    if thread.message_count.unwrap() == 0 {
                        info!("Deleting thread {}", thread.name);
                        if let Err(e) = thread.delete(self.http.clone()).await {
                            error!("Attempted to delete thread {} but failed: {:#?}", thread.name, e);
                        }
                    }
                }
            }
        }

        info!(%cutoff_time, "Sweeping expired messages.");

        let message_ids: Vec<MessageId> = self
            .message_stream()
            .try_take_while(|message| future::ready(Ok(message.timestamp.deref() < &cutoff_time)))
            .filter_map(|m_result| async  {
                let message = m_result.unwrap();
                let dry_run = self.dry_run;
                let success_count = success_count.clone();
                match dry_run {
                    true => {
                        debug!(%message.id, "Skipped adding message to delete queue due to dry run.");
                        None
                    }
                    false => {
                        let success_count = success_count;
                        success_count.fetch_add(1, Ordering::SeqCst);
                        if message.pinned {
                            debug!(%message.id, "message is pinned, skipping delete.");
                            return None;
                        }
                        Some(message.id)
                    }
                }
            })
            .collect()
            .await;

        let success_count = success_count.load(Ordering::SeqCst);

        let total_messages = message_ids.len();
        debug!("Preparing to issue deletes for {total_messages} messages.");
        for chunk in message_ids.chunks(100) {
            debug!("Issuing chunk delete for {} messages.", chunk.len());
            if let Err(e) = self
                .channel_id
                .delete_messages(self.http.clone(), chunk)
                .await
            {
                if let Some(channel) = self.log_channel {
                    let _ = send_message_to_channel(
                    self.http.clone(),
                    self.guild_id,
                    channel,
                    format!("Unable to delete messages: {:#?}", e),
                )
                .await;
                }
                return;
            }
        }

        self.stats.runs += 1;
        self.stats.last_run = success_count;
        self.stats.all_runs += success_count;
        self.stats_tx
            .send(self.stats.clone())
            .expect("failed to update stats");
    }

    /// Returns a stream of messages for the sweeper's channel. The stream takes care of paginating
    /// the response.
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

    /// Load a page of messages from discord and sort them by timestamp.
    async fn load_messages(&self, cursor: impl Into<MessageId>) -> serenity::Result<Vec<Message>> {
        let mut messages = self
            .channel_id
            .messages(&self.http, |b| b.after(cursor.into()).limit(25))
            .await?;

        messages.sort_by_key(|m| m.timestamp);
        Ok(messages)
    }
}
/// send a message to a specified channel.
async fn send_message_to_channel(
    http: Arc<Http>,
    _guild: GuildId,
    channel: ChannelId,
    message: String,
) -> anyhow::Result<()> {
    let formatted_message = MessageBuilder::new().push(message).build();
    if let Err(e) = channel.say(&http, formatted_message).await {
        error!("Couldn't use channel.say in eventhandler: {e}")
    };

    Ok(())
}
