use crate::media::klv_parser::KlvField;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct VideoFrame {
    pub data: Arc<Vec<u8>>,
    pub timestamp: u64,
    pub is_keyframe: bool,
}

#[derive(Clone, Debug)]
pub struct MetadataFrame {
    pub timestamp: u64,
    pub fields: Arc<Vec<KlvField>>,
}
