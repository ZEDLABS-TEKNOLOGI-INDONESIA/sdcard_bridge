use anyhow::{bail, Result};
use crc32fast::Hasher;

pub const HEADER_LEN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Ping = 0x01,
    SdInfo = 0x02,
    ListDir = 0x03,
    ReadFileInit = 0x04,
    ReadChunk = 0x05,
    WriteFileInit = 0x06,
    WriteChunk = 0x07,
    RawBlockWrite = 0x08,
    RawBlockRead = 0x09,
    FlashFinalize = 0x0A,
    StatusReply = 0xFF,
}

impl Cmd {
    pub fn from_u8(v: u8) -> Result<Self> {
        Ok(match v {
            0x01 => Cmd::Ping,
            0x02 => Cmd::SdInfo,
            0x03 => Cmd::ListDir,
            0x04 => Cmd::ReadFileInit,
            0x05 => Cmd::ReadChunk,
            0x06 => Cmd::WriteFileInit,
            0x07 => Cmd::WriteChunk,
            0x08 => Cmd::RawBlockWrite,
            0x09 => Cmd::RawBlockRead,
            0x0A => Cmd::FlashFinalize,
            0xFF => Cmd::StatusReply,
            other => bail!("unknown cmd byte: 0x{:02X}", other),
        })
    }
}

/// A single framed message: [CMD:1B][LEN:4B][PAYLOAD:LEN][CRC32:4B]
#[derive(Debug, Clone)]
pub struct Frame {
    pub cmd: Cmd,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(cmd: Cmd, payload: Vec<u8>) -> Self {
        Self { cmd, payload }
    }

    /// Serialize this frame to bytes ready to write to the serial port.
    pub fn encode(&self) -> Vec<u8> {
        let len = self.payload.len() as u32;
        let mut header = Vec::with_capacity(HEADER_LEN);
        header.push(self.cmd as u8);
        header.extend_from_slice(&len.to_be_bytes());

        let mut hasher = Hasher::new();
        hasher.update(&header);
        hasher.update(&self.payload);
        let crc = hasher.finalize();

        let mut out = header;
        out.extend_from_slice(&self.payload);
        out.extend_from_slice(&crc.to_be_bytes());
        out
    }

    /// Verify a received frame's CRC given the raw header+payload+crc bytes already split.
    pub fn verify_crc(cmd_byte: u8, len_bytes: &[u8; 4], payload: &[u8], crc_bytes: &[u8; 4]) -> bool {
        let mut header = vec![cmd_byte];
        header.extend_from_slice(len_bytes);

        let mut hasher = Hasher::new();
        hasher.update(&header);
        hasher.update(payload);
        let computed = hasher.finalize();

        computed.to_be_bytes() == *crc_bytes
    }
}

/// Parsed status reply payload (cmd 0xFF): [status:1B][msg bytes...]
pub struct StatusReply {
    pub ok: bool,
    pub message: String,
}

impl StatusReply {
    pub fn parse(payload: &[u8]) -> Result<Self> {
        if payload.is_empty() {
            bail!("empty status payload");
        }
        let ok = payload[0] == 0x00;
        let message = String::from_utf8_lossy(&payload[1..]).to_string();
        Ok(Self { ok, message })
    }
}
