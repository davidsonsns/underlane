//! Desired hostname availability, independent of the shared connector process.

use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::config::IngressRule;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteKey {
    pub hostname: String,
    pub path: Option<String>,
    pub service: String,
}

impl RouteKey {
    pub fn from_rule(rule: &IngressRule) -> Option<Self> {
        Some(Self { hostname: rule.hostname.clone()?, path: rule.path.clone(), service: rule.service.clone() })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "routes", rename_all = "snake_case")]
pub enum RouteSelection {
    #[default]
    All,
    Only(Vec<RouteKey>),
}

impl RouteSelection {
    pub fn enables(&self, key: Option<&RouteKey>) -> bool {
        match self {
            Self::All => true,
            Self::Only(keys) => key.is_some_and(|key| keys.contains(key)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteTarget {
    pub key: RouteKey,
    pub run: u64,
    pub config_revision: u64,
}

pub type SharedSelection = Arc<RwLock<RouteSelection>>;

#[derive(Clone)]
pub struct RouteGate {
    pub selection: SharedSelection,
    pub key: Option<RouteKey>,
}

impl RouteGate {
    pub fn enabled(&self) -> bool {
        self.selection.read().unwrap().enables(self.key.as_ref())
    }
}
