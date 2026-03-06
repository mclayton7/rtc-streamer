use std::sync::RwLock;

#[derive(Clone, Debug)]
pub struct H264Params {
    pub sps: Vec<u8>,
    pub pps: Vec<u8>,
}

pub struct H264Parser {
    params: RwLock<Option<H264Params>>,
}

impl H264Parser {
    pub fn new() -> Self {
        Self {
            params: RwLock::new(None),
        }
    }

    pub fn get_params(&self) -> Option<H264Params> {
        self.params.read().unwrap().clone()
    }

    /// Extract SPS/PPS parameter sets from an H.264 Annex B byte stream.
    pub fn update_params(&self, data: &[u8]) {
        let mut sps = None;
        let mut pps = None;

        for nal in Self::find_nal_units(data) {
            match Self::get_nal_type(nal) {
                7 => sps = Some(nal.to_vec()),
                8 => pps = Some(nal.to_vec()),
                _ => {}
            }
        }

        if let (Some(sps), Some(pps)) = (sps, pps) {
            *self.params.write().unwrap() = Some(H264Params { sps, pps });
        }
    }

    /// Find NAL units in an Annex B byte stream.
    /// Returns slices into the input data, each starting at the NAL header byte
    /// (after the start code prefix).
    pub fn find_nal_units(data: &[u8]) -> Vec<&[u8]> {
        let mut units = Vec::new();
        let mut i = 0;
        let mut nal_start: Option<usize> = None;

        while i + 2 < data.len() {
            if data[i] != 0 || data[i + 1] != 0 {
                i += 1;
                continue;
            }

            let sc_len = if i + 3 < data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                4
            } else if data[i + 2] == 1 {
                3
            } else {
                i += 1;
                continue;
            };

            if let Some(start) = nal_start {
                if start < i {
                    units.push(&data[start..i]);
                }
            }
            nal_start = Some(i + sc_len);
            i += sc_len;
        }

        // Last NAL unit extends to end of data
        if let Some(start) = nal_start {
            if start < data.len() {
                units.push(&data[start..]);
            }
        }

        units
    }

    #[inline]
    pub fn get_nal_type(nal: &[u8]) -> u8 {
        if nal.is_empty() {
            return 0;
        }
        nal[0] & 0x1F
    }

    /// Check if an H.264 Annex B byte stream contains an IDR (keyframe) NAL unit.
    pub fn is_keyframe(data: &[u8]) -> bool {
        Self::find_nal_units(data)
            .iter()
            .any(|nal| Self::get_nal_type(nal) == 5)
    }
}
