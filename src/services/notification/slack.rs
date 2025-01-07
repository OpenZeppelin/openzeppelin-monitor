use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use std::collections::HashMap;

use super::{NotificationError, Notifier};
use crate::models::TriggerTypeConfig;

pub struct SlackNotifier {
    webhook_url: String,
    title: String,
    body_template: String,
    client: Client,
}

#[derive(Serialize)]
struct SlackMessage {
    text: String,
}

impl SlackNotifier {
    pub fn new(webhook_url: String, title: String, body_template: String) -> Self {
        Self {
            webhook_url,
            title,
            body_template,
            client: Client::new(),
        }
    }

    pub fn format_message(&self, variables: &HashMap<String, String>) -> String {
        let mut message = self.body_template.clone();
        for (key, value) in variables {
            message = message.replace(&format!("${{{}}}", key), value);
        }
        format!("*{}*\n\n{}", self.title, message)
    }

    pub fn from_config(config: &TriggerTypeConfig) -> Option<Self> {
        match config {
            TriggerTypeConfig::Slack {
                webhook_url,
                title,
                body,
            } => Some(Self {
                webhook_url: webhook_url.clone(),
                title: title.clone(),
                body_template: body.clone(),
                client: Client::new(),
            }),
            _ => None,
        }
    }
}

#[async_trait]
impl Notifier for SlackNotifier {
    async fn notify(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let payload = SlackMessage {
            text: message.to_string(),
        };

        self.client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| Box::new(NotificationError::network_error(e.to_string())))?;

        Ok(())
    }
}
