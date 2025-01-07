use std::collections::HashMap;

use async_trait::async_trait;

mod error;
mod slack;

pub use error::NotificationError;
pub use slack::SlackNotifier;

use crate::models::TriggerTypeConfig;

#[async_trait]
pub trait Notifier {
    async fn notify(&self, message: &str) -> Result<(), Box<dyn std::error::Error>>;
}

pub struct NotificationService;

impl NotificationService {
    pub fn new() -> Self {
        NotificationService
    }

    pub async fn execute(
        &self,
        config: &TriggerTypeConfig,
        variables: HashMap<String, String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match config {
            TriggerTypeConfig::Slack {
                webhook_url,
                title,
                body,
            } => {
                let notifier = SlackNotifier::new(webhook_url.clone(), title.clone(), body.clone());
                notifier
                    .notify(&notifier.format_message(&variables))
                    .await?;
            }
            TriggerTypeConfig::Webhook { .. } => {
                // TODO: Implement webhook notifier
                todo!("Implement webhook notification")
            }
            TriggerTypeConfig::Script { .. } => {
                // TODO: Implement script notifier
                todo!("Implement script execution")
            }
        }
        Ok(())
    }
}
