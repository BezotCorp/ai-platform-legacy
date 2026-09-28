use std::sync::{Arc, atomic::AtomicBool};

use crate::sqlite::{Readers, Writer};

pub(crate) struct DatabaseInner {
    pub(crate) writer: Writer,
    pub(crate) readers: Readers,
    pub(crate) healthy: Arc<AtomicBool>,
    pub(crate) closing: AtomicBool,
}
