use reqwest::Client;
use tracing::{error, info};

/// Fire the webhook to trigger the garage door
pub async fn fire_webhook(url: &str) -> anyhow::Result<()> {
    if url.is_empty() {
        error!("Webhook URL is empty, cannot fire webhook");
        return Err(anyhow::anyhow!("Webhook URL not configured"));
    }

    info!("Firing webhook: {}", url);

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .body(r#"{"action": "open_garage"}"#)
        .send()
        .await?;

    let status = response.status();
    if status.is_success() {
        info!("Webhook fired successfully (status: {})", status);
    } else {
        error!("Webhook returned error status: {}", status);
        return Err(anyhow::anyhow!("Webhook returned status {}", status));
    }

    Ok(())
}
