use anyhow::{bail, Result};
use s3::creds::Credentials;
use s3::error::S3Error;
use s3::{Bucket, Region};
use serenity::model::guild::Guild;
use tracing::debug;

pub(crate) async fn upload_emoji(message: &String) -> Result<()> {
    debug!("{message}");
    let args: Vec<&str> = message.split(" ").collect();
    debug!("emoji: {}", args[1]);
    let bucket = Bucket::new(
        "emoji",
        Region::Custom {
            region: "us-east-1".to_owned(),
            endpoint: "https://s3.vsix.me:9000".to_owned(),
        },
        Credentials::default()?,
    )?
    .with_path_style();
    let file_list = match bucket
        .list(format! {"{}/", args[1]}, Some("/".to_owned()))
        .await
    {
        Ok(s) => s,
        Err(e) => {
            bail!("{}", e);
        }
    };
    let link = format!(
        "https://s3.vsix.me:9000/emoji/{}",
        file_list[0].contents[0].key
    );
    debug!("{link}");
    bail!("not implemented yet");
}
