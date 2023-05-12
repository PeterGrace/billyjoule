use anyhow::{bail, Result};
use tracing::debug;

pub(crate) async fn upload_emoji(message: &String) -> Result<()> {
    debug!("{message}");
    bail!("Not implemented yet.");
}
