use std::{collections::HashMap, path::Path};

use crate::{
    models::{ConfigLoader, Trigger},
    repositories::error::RepositoryError,
};
pub struct TriggerRepository {
    pub triggers: HashMap<String, Trigger>,
}

impl TriggerRepository {
    pub fn new(path: Option<&Path>) -> Result<Self, RepositoryError> {
        let triggers = Trigger::load_all(path)
            .map_err(|e| RepositoryError::load_error(format!("Failed to load triggers: {}", e)))?;
        Ok(TriggerRepository { triggers })
    }
}

pub trait TriggerRepositoryTrait {
    fn load_all(&self, path: Option<&Path>) -> Result<HashMap<String, Trigger>, RepositoryError>;
    fn get(&self, trigger_id: &str) -> Option<Trigger>;
    fn get_all(&self) -> HashMap<String, Trigger>;
}

impl TriggerRepositoryTrait for TriggerRepository {
    fn load_all(&self, path: Option<&Path>) -> Result<HashMap<String, Trigger>, RepositoryError> {
        Trigger::load_all(path).map_err(|e| RepositoryError::load_error(format!("Failed: {}", e)))
    }

    fn get(&self, trigger_id: &str) -> Option<Trigger> {
        self.triggers.get(trigger_id).cloned()
    }

    fn get_all(&self) -> HashMap<String, Trigger> {
        self.triggers.clone()
    }
}

pub struct TriggerService<T: TriggerRepositoryTrait> {
    repository: T,
}

impl<T: TriggerRepositoryTrait> TriggerService<T> {
    pub fn new(path: Option<&Path>) -> Result<TriggerService<TriggerRepository>, RepositoryError> {
        let repository = TriggerRepository::new(path)?;
        Ok(TriggerService { repository })
    }

    pub fn new_with_repository(repository: T) -> Result<Self, RepositoryError> {
        Ok(TriggerService { repository })
    }

    pub fn new_with_path(
        path: Option<&Path>,
    ) -> Result<TriggerService<TriggerRepository>, RepositoryError> {
        let repository = TriggerRepository::new(path)?;
        Ok(TriggerService { repository })
    }

    pub fn get(&self, trigger_id: &str) -> Option<Trigger> {
        self.repository.get(trigger_id)
    }

    pub fn get_all(&self) -> HashMap<String, Trigger> {
        self.repository.get_all()
    }
}
