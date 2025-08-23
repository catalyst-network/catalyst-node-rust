use std::collections::HashMap;
use crate::{ServiceType, CatalystService};

pub struct ServiceRegistry {
    services: HashMap<ServiceType, String>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self { services: HashMap::new() }
    }

    pub fn register(&mut self, service_type: ServiceType, name: String) {
        self.services.insert(service_type, name);
    }

    pub fn get(&self, service_type: &ServiceType) -> Option<&String> {
        self.services.get(service_type)
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}