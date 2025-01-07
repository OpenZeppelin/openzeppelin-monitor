use async_trait::async_trait;
use glob::glob;
use std::error::Error;
use std::path::PathBuf;

use crate::models::BlockType;

#[async_trait]
pub trait BlockStorage {
    async fn get_last_processed_block(
        &self,
        network_id: &str,
    ) -> Result<Option<u64>, Box<dyn Error>>;
    async fn save_last_processed_block(
        &self,
        network_id: &str,
        block: u64,
    ) -> Result<(), Box<dyn Error>>;
    async fn save_blocks(
        &self,
        network_id: &str,
        blocks: &Vec<BlockType>,
    ) -> Result<(), Box<dyn Error>>;
    async fn delete_blocks(&self, network_id: &str) -> Result<(), Box<dyn Error>>;
}

pub struct FileBlockStorage {
    storage_path: PathBuf,
}

impl FileBlockStorage {
    pub fn new() -> Self {
        FileBlockStorage {
            storage_path: PathBuf::from("data"),
        }
    }
}

#[async_trait]
impl BlockStorage for FileBlockStorage {
    async fn get_last_processed_block(
        &self,
        network_id: &str,
    ) -> Result<Option<u64>, Box<dyn Error>> {
        let file_path = self
            .storage_path
            .join(format!("{}_last_block.txt", network_id));

        if !file_path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(file_path).await?;
        let block_number = content.trim().parse()?;
        Ok(Some(block_number))
    }

    async fn save_last_processed_block(
        &self,
        network_id: &str,
        block: u64,
    ) -> Result<(), Box<dyn Error>> {
        let file_path = self
            .storage_path
            .join(format!("{}_last_block.txt", network_id));
        tokio::fs::write(file_path, block.to_string()).await?;
        Ok(())
    }

    async fn save_blocks(
        &self,
        network_slug: &str,
        blocks: &Vec<BlockType>,
    ) -> Result<(), Box<dyn Error>> {
        let file_path = self.storage_path.join(format!(
            "{}_blocks_{}.json",
            network_slug,
            chrono::Utc::now().timestamp()
        ));
        let json = serde_json::to_string(blocks)?;
        tokio::fs::write(file_path, json).await?;
        Ok(())
    }

    async fn delete_blocks(&self, network_slug: &str) -> Result<(), Box<dyn Error>> {
        let pattern = self
            .storage_path
            .join(format!("{}_blocks_*.json", network_slug))
            .to_string_lossy()
            .to_string();

        for entry in glob(&pattern)? {
            if let Ok(path) = entry {
                tokio::fs::remove_file(path).await?;
            }
        }
        Ok(())
    }
}
