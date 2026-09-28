use anyhow::Result;
use rusqlite::Connection;

use crate::sqlite::OperationResult;

pub(crate) type Operation =
    Box<dyn FnOnce(&Connection) -> Result<OperationResult> + Send + 'static>;
