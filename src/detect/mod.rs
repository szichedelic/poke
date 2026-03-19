pub mod scraper;
pub mod structured;

use crate::models::AgentStatus;

pub trait Detector {
    fn scan(&self) -> Vec<AgentStatus>;
}
