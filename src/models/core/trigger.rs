use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Trigger {
    pub name: String,
    pub trigger_type: TriggerType,
    pub config: TriggerTypeConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TriggerType {
    Slack,
    Webhook,
    Script,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TriggerTypeConfig {
    Slack {
        webhook_url: String,
        title: String,
        body: String,
    },
    Webhook {
        url: String,
        method: String,
        headers: Option<std::collections::HashMap<String, String>>,
    },
    Script {
        path: String,
        args: Vec<String>,
    },
}
