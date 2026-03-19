pub mod scraper;

use crate::models::AgentStatus;

pub trait Detector {
    fn scan(&self) -> Vec<AgentStatus>;
}
