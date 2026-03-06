use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct VideoFrame {
    pub data: Arc<Vec<u8>>,
    pub timestamp: u64,
    pub dts: u64,
    pub is_keyframe: bool,
}

#[derive(Clone, Debug)]
pub struct AudioFrame {
    pub data: Arc<Vec<u8>>,
    pub timestamp: u64,
}
