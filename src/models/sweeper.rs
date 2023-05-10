use async_stream::try_stream;
use chrono::{DateTime, Duration, Utc};
use serenity::futures::{Stream, TryStreamExt};
use serenity::http::Http;
use serenity::model::channel::Message;
use serenity::model::id::{ChannelId, MessageId};
use serenity::prelude::TypeMapKey;
use std::future;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use tracing::log::warn;
use tracing::{debug, error, info};

pub(crate) async fn run_sweeper(mut sweeper: Sweeper, mut ready: mpsc::Receiver<()>) {
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

        tokio::time::sleep(Duration::hours(1).to_std().expect("1 hour is in range")).await;
    }
}

pub(crate) struct Sweeper {
    http: Arc<Http>,
    channel_id: ChannelId,
    max_message_age: Duration,
    dry_run: bool,

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

        (
            Sweeper {
                http: Arc::new(http),
                channel_id,
                max_message_age,
                dry_run,
                stats,
                stats_tx: tx,
            },
            rx,
        )
    }

    async fn sweep_messages(&mut self) {
        let cutoff_time = Utc::now() - self.max_message_age;

        let success_count = Arc::new(AtomicU32::new(0));
        let delete_message_ids: Arc<Mutex<Vec<MessageId>>> = Arc::new(Mutex::new(vec![]));
        info!(%cutoff_time, "Sweeping expired messages.");

        let res = self
            .message_stream()
            .try_take_while(|message| future::ready(Ok(message.timestamp.deref() < &cutoff_time)))
            .try_for_each(|message| {
                // Make the borrow checker happy.
                let message_id = message.id;
                let dry_run = self.dry_run;
                let success_count = success_count.clone();
                let delete_message_ids = delete_message_ids.clone();

                async move {
                    debug!(dry_run, %message_id,  "Found expired message.");

                    if message.pinned {
                        debug!(%message_id, "message is pinned, skipping.");
                        return Ok(());
                    }

                    // if we're in dry run mode, we're skipping the message.
                    match dry_run {
                        true => {}
                        false => {
                            match delete_message_ids.lock() {
                                Ok(mut vec) => {
                                    if vec.len() < 100 {
                                        // we can only delete up to 100 bulk messages at a time.
                                        vec.push(message_id)
                                    }
                                },
                                Err(e) => {
                                    warn!("Unable to lock message id array, message may remain after sweep: {:?}", e);
                                }
                            }
                            success_count.fetch_add(1, Ordering::SeqCst);
                        },
                    }
                    Ok(())
                }

            })
            .await;

        let success_count = success_count.load(Ordering::SeqCst);
        match res {
            Ok(()) => {
                info!(count = success_count, "Successfully swept messages.");
            }
            Err(error) => {
                error!(%error, "Failed to sweep messages");
            }
        }

        let messages_to_delete = delete_message_ids.lock().unwrap().to_vec().clone();
        // bulk delete only accepts 2-100 messages.
        if messages_to_delete.len() > 1 {
            if let Err(e) = self
                .channel_id
                .delete_messages(self.http.clone(), messages_to_delete.clone())
                .await
            {
                error!("Unable to delete messages: {:#?}", e);
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
