//! Packet definitions for video and data transport

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Type of packet being transmitted
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum PacketType {
    /// Video frame data (sent via unreliable datagram)
    VideoFrame = 0x01,
    /// Video frame acknowledgment with RTT info
    VideoAck = 0x02,
    /// Input event (reliable stream)
    Input = 0x10,
    /// Clipboard data (reliable stream)
    Clipboard = 0x20,
    /// File transfer chunk (reliable stream)
    FileChunk = 0x30,
    /// Session control message
    SessionControl = 0x40,
    /// Heartbeat/keepalive
    Heartbeat = 0xFF,
}

/// Video codec type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoCodec {
    H264,
    H265,
    VP9,
    AV1,
}

impl Default for VideoCodec {
    fn default() -> Self {
        Self::H264
    }
}

/// Frame type indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameType {
    /// Keyframe (I-frame) - can be decoded independently
    Key,
    /// Delta frame (P-frame) - depends on previous frames
    Delta,
    /// Bidirectional frame (B-frame) - depends on past and future
    Bidirectional,
}

/// Region of the screen that changed (dirty rect)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DirtyRect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn full_screen(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }
}

/// Video packet header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoPacketHeader {
    /// Unique frame identifier
    pub frame_id: u64,
    /// Fragment index within this frame
    pub fragment_index: u16,
    /// Total fragments in this frame
    pub total_fragments: u16,
    /// Timestamp in microseconds (monotonic)
    pub timestamp_us: u64,
    /// Frame type
    pub frame_type: FrameType,
    /// Video codec
    pub codec: VideoCodec,
    /// Screen dimensions
    pub width: u32,
    pub height: u32,
    /// Dirty region (if partial update)
    pub dirty_rect: Option<DirtyRect>,
}

/// Complete video packet with payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoPacket {
    pub header: VideoPacketHeader,
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
}

impl VideoPacket {
    /// Serialize to bytes for transmission
    pub fn to_bytes(&self) -> Result<Bytes, bincode::Error> {
        let encoded = bincode::serialize(self)?;
        Ok(Bytes::from(encoded))
    }

    /// Deserialize from received bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }
}

/// Acknowledgment for received video frame
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoAck {
    /// Frame ID being acknowledged
    pub frame_id: u64,
    /// Fragments successfully received (bitmask)
    pub received_fragments: u64,
    /// Measured RTT in microseconds
    pub rtt_us: u64,
    /// Client-side decode time in microseconds
    pub decode_time_us: u64,
    /// Client-side render time in microseconds
    pub render_time_us: u64,
    /// Current buffer occupancy (frames)
    pub buffer_occupancy: u8,
}

/// Clipboard content type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardContent {
    Text(String),
    Image {
        width: u32,
        height: u32,
        #[serde(with = "serde_bytes")]
        rgba_data: Vec<u8>,
    },
    Files(Vec<String>),
}

/// Clipboard packet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardPacket {
    pub content: ClipboardContent,
    pub timestamp_us: u64,
}

/// File transfer packet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChunkPacket {
    /// Unique transfer ID
    pub transfer_id: uuid::Uuid,
    /// File name
    pub filename: String,
    /// Total file size
    pub total_size: u64,
    /// Chunk offset
    pub offset: u64,
    /// Chunk data
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
    /// Is this the last chunk?
    pub is_final: bool,
}

mod serde_bytes {
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        serde::Deserialize::deserialize(deserializer)
    }
}
