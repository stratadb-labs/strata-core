//! ConfigureModel command handler.
//!
//! Stores model configuration as a Database extension for use by the search handler.

use std::sync::Arc;

use strata_engine::database::ModelConfig;
use strata_engine::database::ModelConfigState;

use crate::bridge::Primitives;
use crate::{Output, Result};

/// Handle ConfigureModel command: store model endpoint configuration.
pub fn configure_model(
    p: &Arc<Primitives>,
    endpoint: String,
    model: String,
    api_key: Option<String>,
    timeout_ms: Option<u64>,
) -> Result<Output> {
    let state =
        p.db.extension::<ModelConfigState>()
            .map_err(crate::Error::from)?;

    let config = ModelConfig {
        endpoint,
        model,
        api_key,
        timeout_ms: timeout_ms.unwrap_or(5000),
    };

    *state.config.write() = Some(config);

    Ok(Output::Unit)
}
