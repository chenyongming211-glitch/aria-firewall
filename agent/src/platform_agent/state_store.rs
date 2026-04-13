use std::path::PathBuf;

use serde::{de::DeserializeOwned, Serialize};
use tokio::fs;
use tracing::warn;

use super::helpers::temp_path;
use super::ir_types::*;

pub(crate) struct LocalPlatformStateStore {
    pub(crate) root: PathBuf,
}

impl LocalPlatformStateStore {
    pub(crate) fn new
        Self {
            root: base_state_dir.join("platform-agent"),
        }
    }

    pub(crate) async fn load_desired_state
        self.load_json(self.desired_state_path()).await
    }

    pub(crate) async fn save_desired_state
        self.save_json(self.desired_state_path(), state).await
    }

    pub(crate) async fn load_compiled_state
        self.load_json(self.compiled_state_path()).await
    }

    pub(crate) async fn save_compiled_state
        self.save_json(self.compiled_state_path(), state).await
    }

    pub(crate) async fn load_reconcile_plan
        self.load_json(self.reconcile_plan_path()).await
    }

    pub(crate) async fn save_reconcile_plan
        self.save_json(self.reconcile_plan_path(), plan).await
    }

    pub(crate) async fn load_runtime_plan
        self.load_json(self.runtime_plan_path()).await
    }

    pub(crate) async fn save_runtime_plan
        self.save_json(self.runtime_plan_path(), plan).await
    }

    pub(crate) async fn load_runtime_inventory
        self.load_json(self.runtime_inventory_path()).await
    }

    pub(crate) async fn save_runtime_inventory
        self.save_json(self.runtime_inventory_path(), inventory)
            .await
    }

    pub(crate) async fn load_runtime_inventory_diff
        self.load_json(self.runtime_inventory_diff_path()).await
    }

    pub(crate) async fn save_runtime_inventory_diff
        self.save_json(self.runtime_inventory_diff_path(), diff)
            .await
    }

    pub(crate) async fn load_runtime_intent
        self.load_json(self.runtime_intent_path()).await
    }

    pub(crate) async fn save_runtime_intent
        self.save_json(self.runtime_intent_path(), intent).await
    }

    pub(crate) async fn load_runtime_execution_summary
        self.load_json(self.runtime_execution_summary_path()).await
    }

    pub(crate) async fn save_runtime_execution_summary
        &self,
        summary: &RuntimeExecutionSummary,
    ) -> Result<(), String> {
        self.save_json(self.runtime_execution_summary_path(), summary)
            .await
    }

    pub(crate) async fn load_socket_selection_plan
        self.load_json(self.socket_selection_plan_path()).await
    }

    pub(crate) async fn save_socket_selection_plan
        self.save_json(self.socket_selection_plan_path(), plan)
            .await
    }

    pub(crate) fn desired_state_path
        self.root.join("desired-state-cache.json")
    }

    pub(crate) fn compiled_state_path
        self.root.join("compiled-node-state.json")
    }

    pub(crate) fn reconcile_plan_path
        self.root.join("reconcile-plan.json")
    }

    pub(crate) fn runtime_plan_path
        self.root.join("runtime-plan.json")
    }

    pub(crate) fn runtime_inventory_path
        self.root.join("runtime-inventory.json")
    }

    pub(crate) fn runtime_inventory_diff_path
        self.root.join("runtime-inventory-diff.json")
    }

    pub(crate) fn runtime_intent_path
        self.root.join("runtime-intent.json")
    }

    pub(crate) fn runtime_execution_summary_path
        self.root.join("runtime-execution-summary.json")
    }

    pub(crate) fn socket_selection_plan_path
        self.root.join("socket-selection-plan.json")
    }

    pub(crate) async fn load_json
    where
        T: DeserializeOwned,
    {
        let contents = fs::read_to_string(&path).await.ok()?;
        match serde_json::from_str::<T>(&contents) {
            Ok(value) => Some(value),
            Err(error) => {
                warn!(path = %path.display(), error = %error, "failed to decode local platform-agent state");
                None
            }
        }
    }

    pub(crate) async fn save_json
    where
        T: Serialize,
    {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|error| {
                format!(
                    "failed to create platform-agent state directory {}: {error}",
                    parent.display()
                )
            })?;
        }

        let bytes = serde_json::to_vec_pretty(value).map_err(|error| {
            format!(
                "failed to encode platform-agent state {}: {error}",
                path.display()
            )
        })?;
        let tmp_path = temp_path(&path);
        fs::write(&tmp_path, bytes).await.map_err(|error| {
            format!(
                "failed to write platform-agent state {}: {error}",
                tmp_path.display()
            )
        })?;
        fs::rename(&tmp_path, &path).await.map_err(|error| {
            format!(
                "failed to replace platform-agent state {}: {error}",
                path.display()
            )
        })?;
        Ok(())
    }
}
