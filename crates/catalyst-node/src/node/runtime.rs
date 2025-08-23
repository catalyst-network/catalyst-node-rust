use std::time::Duration;
use tokio::time::interval;
use tracing::info;

pub struct NodeRuntime {
    is_running: bool,
}

impl NodeRuntime {
    pub fn new() -> Self {
        Self { is_running: false }
    }

    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.is_running = true;
        info!("🔄 Node runtime started");
        Ok(())
    }

    pub async fn stop(&mut self) {
        self.is_running = false;
        info!("🔄 Node runtime stopped");
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }
}