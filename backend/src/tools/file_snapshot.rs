pub(crate) struct FileSnapshot {
    pub(crate) bytes: Vec<u8>,
    pub(crate) device: u64,
    pub(crate) inode: u64,
    pub(crate) mode: u32,
}

impl FileSnapshot {
    pub(crate) fn matches(&self, other: &Self) -> bool {
        self.device == other.device
            && self.inode == other.inode
            && self.mode == other.mode
            && self.bytes == other.bytes
    }
}
