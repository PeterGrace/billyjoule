use async_stream::try_stream;
use chrono::{DateTime, Duration, Utc};
use serenity::futures::{Stream, TryStreamExt};
use serenity::http::Http;
use serenity::model::channel::Message;
use serenity::model::id::{ChannelId, MessageId};
use std::future;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
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
    started: DateTime<Utc>,
    runs: u32,
    last_run: u32,
    all_runs: u32,
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

        info!(%cutoff_time, "Sweeping expired messages.");
        let res = self
            .message_stream()
            .try_take_while(|message| future::ready(Ok(message.timestamp.deref() < &cutoff_time)))
            .try_for_each(|message| {
                let message_id = message.id;
                let channel_id = self.channel_id;
                let http = self.http.clone();
                let dry_run = self.dry_run;
                let success_count = success_count.clone();

                async move {
                    debug!(dry_run, %message_id,  "Found expired message.");

                    if dry_run {
                        return Ok(());
                    }

                    if !dry_run {
                        channel_id.delete_message(http, message_id).await.map(|_| {
                            success_count.fetch_add(1, Ordering::SeqCst);
                        })
                    } else {
                        Ok(())
                    }
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

        self.stats.runs += 1;
        self.stats.last_run = success_count;
        self.stats.all_runs += success_count;
        self.stats_tx
            .send(self.stats.clone())
            .expect("failed to update stats");
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
            .messages(&self.http, |b| b.after(cursor.into()).limit(25))
            .await?;

        messages.sort_by_key(|m| m.timestamp);
        Ok(messages)
    }
}
