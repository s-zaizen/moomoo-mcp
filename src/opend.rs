use std::{
    fmt, io,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use aes::Aes128;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use prost::Message;
use rsa::{
    RsaPrivateKey, RsaPublicKey, pkcs1::DecodeRsaPrivateKey, pkcs1v15::Pkcs1v15Encrypt,
    rand_core::OsRng, traits::PublicKeyParts,
};
use sha1::{Digest, Sha1};
use thiserror::Error;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::Mutex,
    time::sleep,
};
use tracing::{debug, warn};

use crate::{
    config::Config,
    proto::{common, get_global_state, init_connect, keep_alive},
};

const HEADER_LEN: usize = 44;
const PROTO_FMT_PROTOBUF: u8 = 0;
const API_PROTO_VER: u8 = 0;
const PROTO_ID_INIT_CONNECT: u32 = 1001;
const PROTO_ID_GET_GLOBAL_STATE: u32 = 1002;
const PROTO_ID_KEEP_ALIVE: u32 = 1004;

type Aes128CbcEnc = cbc::Encryptor<Aes128>;
type Aes128CbcDec = cbc::Decryptor<Aes128>;

#[derive(Debug, Error)]
pub enum MoomooError {
    #[error("{message}")]
    Api {
        message: String,
        err_code: Option<i32>,
    },
    #[error("{message}")]
    Config { message: String },
    #[error("{message}")]
    InvalidParam { message: String },
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("protobuf decode error: {0}")]
    Protobuf(#[from] prost::DecodeError),
    #[error("{message}")]
    Protocol { message: String },
    #[error("{message}")]
    Crypto { message: String },
}

impl From<MoomooError> for rmcp::model::ErrorData {
    fn from(value: MoomooError) -> Self {
        match value {
            MoomooError::InvalidParam { message } | MoomooError::Config { message } => {
                Self::invalid_params(message, None)
            }
            other => Self::internal_error(other.to_string(), None),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MoomooClient {
    config: Config,
    session: Arc<Mutex<Option<Arc<Session>>>>,
    serial: Arc<AtomicU32>,
}

impl MoomooClient {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            session: Arc::new(Mutex::new(None)),
            serial: Arc::new(AtomicU32::new(2)),
        }
    }

    pub async fn query<Req, Resp>(&self, proto_id: u32, request: &Req) -> Result<Resp, MoomooError>
    where
        Req: Message,
        Resp: Message + Default,
    {
        let session = self.ensure_session().await?;
        let serial = self.next_serial();
        match session.roundtrip(proto_id, serial, request).await {
            Ok(response) => Ok(response),
            Err(error) if session.is_broken() => {
                warn!("request failed on stale session, reconnecting once: {error}");
                self.invalidate_session().await;
                let session = self.ensure_session().await?;
                let serial = self.next_serial();
                session.roundtrip(proto_id, serial, request).await
            }
            Err(error) => Err(error),
        }
    }

    pub async fn prepare_command(&self) -> Result<CommandContext, MoomooError> {
        let session = self.ensure_session().await?;
        let conn_id = session.conn_id().await;
        Ok(CommandContext {
            session,
            serial: self.next_serial(),
            conn_id,
        })
    }

    pub async fn execute_command<Req, Resp>(
        &self,
        context: &CommandContext,
        proto_id: u32,
        request: &Req,
    ) -> Result<Resp, MoomooError>
    where
        Req: Message,
        Resp: Message + Default,
    {
        context
            .session
            .roundtrip(proto_id, context.serial, request)
            .await
    }

    async fn ensure_session(&self) -> Result<Arc<Session>, MoomooError> {
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.as_ref() {
            if !session.is_broken() {
                return Ok(session.clone());
            }
        }

        if let Some(old) = guard.take() {
            old.close();
        }

        let session = Arc::new(Session::connect(self.config.clone()).await?);
        Session::spawn_keepalive(session.clone(), self.serial.clone());
        *guard = Some(session.clone());
        Ok(session)
    }

    async fn invalidate_session(&self) {
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.take() {
            session.close();
        }
    }

    fn next_serial(&self) -> u32 {
        self.serial.fetch_add(1, Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct CommandContext {
    pub session: Arc<Session>,
    pub serial: u32,
    pub conn_id: u64,
}

#[derive(Debug)]
pub struct Session {
    state: Mutex<SessionState>,
    rsa_key: Option<RsaPrivateKey>,
    broken: AtomicBool,
    closed: AtomicBool,
}

#[derive(Debug)]
struct SessionState {
    stream: TcpStream,
    conn_id: u64,
    encrypt: bool,
    aes_key: Option<[u8; 16]>,
    aes_iv: Option<[u8; 16]>,
    keep_alive_interval: Duration,
}

#[derive(Clone, Copy, Debug)]
struct PacketHeader {
    proto_id: u32,
    proto_fmt: u8,
    proto_ver: u8,
    serial_no: u32,
    body_len: u32,
    sha1: [u8; 20],
}

impl Session {
    async fn connect(config: Config) -> Result<Self, MoomooError> {
        let stream = TcpStream::connect((config.host.as_str(), config.port)).await?;
        let rsa_key = if config.use_encryption {
            Some(
                RsaPrivateKey::from_pkcs1_pem(&config.rsa_private_key_pem()?).map_err(|error| {
                    MoomooError::Crypto {
                        message: format!("failed to parse RSA private key: {error}"),
                    }
                })?,
            )
        } else {
            None
        };

        let mut session = Self {
            state: Mutex::new(SessionState {
                stream,
                conn_id: 0,
                encrypt: config.use_encryption,
                aes_key: None,
                aes_iv: None,
                keep_alive_interval: Duration::from_secs(8),
            }),
            rsa_key,
            broken: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        };

        session.init(config).await?;
        Ok(session)
    }

    fn spawn_keepalive(session: Arc<Self>, serial: Arc<AtomicU32>) {
        tokio::spawn(async move {
            loop {
                let interval = match session.keep_alive_interval().await {
                    Some(interval) => interval,
                    None => break,
                };
                sleep(interval).await;

                if session.closed.load(Ordering::Relaxed) {
                    break;
                }

                let request = keep_alive::Request {
                    c2s: keep_alive::C2s {
                        time: now_utc_seconds(),
                    },
                };

                let result: Result<keep_alive::Response, MoomooError> = session
                    .roundtrip(
                        PROTO_ID_KEEP_ALIVE,
                        serial.fetch_add(1, Ordering::Relaxed),
                        &request,
                    )
                    .await;
                if let Err(error) = result {
                    warn!("keepalive failed: {error}");
                    session.broken.store(true, Ordering::Relaxed);
                    break;
                }
            }
        });
    }

    async fn roundtrip<Req, Resp>(
        &self,
        proto_id: u32,
        serial: u32,
        request: &Req,
    ) -> Result<Resp, MoomooError>
    where
        Req: Message,
        Resp: Message + Default,
    {
        if self.closed.load(Ordering::Relaxed) {
            return Err(MoomooError::Protocol {
                message: "session is closed".to_string(),
            });
        }

        let mut state = self.state.lock().await;
        let plaintext = request.encode_to_vec();
        let body = self.encode_body(proto_id, &plaintext, &state)?;
        let header = encode_header(proto_id, serial, body.len() as u32, sha1_digest(&plaintext));

        if let Err(error) = state.stream.write_all(&header).await {
            self.broken.store(true, Ordering::Relaxed);
            return Err(MoomooError::Io(error));
        }
        if let Err(error) = state.stream.write_all(&body).await {
            self.broken.store(true, Ordering::Relaxed);
            return Err(MoomooError::Io(error));
        }
        if let Err(error) = state.stream.flush().await {
            self.broken.store(true, Ordering::Relaxed);
            return Err(MoomooError::Io(error));
        }

        let mut raw_header = [0_u8; HEADER_LEN];
        if let Err(error) = state.stream.read_exact(&mut raw_header).await {
            self.broken.store(true, Ordering::Relaxed);
            return Err(MoomooError::Io(error));
        }
        let response_header = decode_header(&raw_header)?;
        let mut response_body = vec![0_u8; response_header.body_len as usize];
        if let Err(error) = state.stream.read_exact(&mut response_body).await {
            self.broken.store(true, Ordering::Relaxed);
            return Err(MoomooError::Io(error));
        }

        if response_header.proto_id != proto_id {
            return Err(MoomooError::Protocol {
                message: format!(
                    "unexpected proto id in response: expected {proto_id}, got {}",
                    response_header.proto_id
                ),
            });
        }

        let plaintext = self.decode_body(proto_id, &response_body, &response_header, &state)?;
        Resp::decode(plaintext.as_slice()).map_err(MoomooError::from)
    }

    async fn conn_id(&self) -> u64 {
        self.state.lock().await.conn_id
    }

    async fn keep_alive_interval(&self) -> Option<Duration> {
        if self.closed.load(Ordering::Relaxed) || self.broken.load(Ordering::Relaxed) {
            return None;
        }

        Some(self.state.lock().await.keep_alive_interval)
    }

    fn is_broken(&self) -> bool {
        self.broken.load(Ordering::Relaxed) || self.closed.load(Ordering::Relaxed)
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
    }

    async fn init(&mut self, config: Config) -> Result<(), MoomooError> {
        let request = init_connect::Request {
            c2s: init_connect::C2s {
                client_ver: config.client_ver,
                client_id: config.client_id,
                recv_notify: Some(config.recv_notify),
                packet_enc_algo: Some(if config.use_encryption {
                    common::PacketEncAlgo::AesCbc as i32
                } else {
                    common::PacketEncAlgo::None as i32
                }),
                push_proto_fmt: Some(common::ProtoFmt::Protobuf as i32),
                programming_language: Some("Rust".to_string()),
            },
        };

        let response: init_connect::Response =
            self.roundtrip(PROTO_ID_INIT_CONNECT, 1, &request).await?;
        if response.ret_type != 0 {
            return Err(MoomooError::Api {
                message: response.ret_msg.unwrap_or_else(|| {
                    format!("InitConnect failed with retType={}", response.ret_type)
                }),
                err_code: response.err_code,
            });
        }

        let info = response.s2c.ok_or_else(|| MoomooError::Protocol {
            message: "InitConnect response missing s2c".to_string(),
        })?;

        let aes_key = parse_fixed_16(&info.conn_aes_key, "connAESKey")?;
        let aes_iv = info
            .aes_cb_civ
            .as_deref()
            .map(|value| parse_fixed_16(value, "aesCBCiv"))
            .transpose()?;

        let mut state = self.state.lock().await;
        state.conn_id = info.conn_id;
        state.encrypt = config.use_encryption;
        state.aes_key = Some(aes_key);
        state.aes_iv = aes_iv;
        state.keep_alive_interval =
            Duration::from_secs(((info.keep_alive_interval as f64) * 0.8).max(1.0) as u64);
        debug!(
            conn_id = state.conn_id,
            keep_alive_interval = ?state.keep_alive_interval,
            "InitConnect succeeded"
        );
        Ok(())
    }

    fn encode_body(
        &self,
        proto_id: u32,
        plaintext: &[u8],
        state: &SessionState,
    ) -> Result<Vec<u8>, MoomooError> {
        if proto_id == PROTO_ID_INIT_CONNECT {
            if state.encrypt {
                let rsa_key = self.rsa_key.as_ref().ok_or_else(|| MoomooError::Crypto {
                    message: "encryption requested but RSA private key is missing".to_string(),
                })?;
                return rsa_encrypt(plaintext, rsa_key);
            }
            return Ok(plaintext.to_vec());
        }

        if !state.encrypt {
            return Ok(plaintext.to_vec());
        }

        let key = state.aes_key.ok_or_else(|| MoomooError::Crypto {
            message: "AES key missing for encrypted session".to_string(),
        })?;
        let iv = state.aes_iv.ok_or_else(|| MoomooError::Crypto {
            message: "AES IV missing for encrypted session".to_string(),
        })?;
        aes_encrypt(&key, &iv, plaintext)
    }

    fn decode_body(
        &self,
        proto_id: u32,
        ciphertext: &[u8],
        header: &PacketHeader,
        state: &SessionState,
    ) -> Result<Vec<u8>, MoomooError> {
        let plaintext = if proto_id == PROTO_ID_INIT_CONNECT {
            if state.encrypt {
                let rsa_key = self.rsa_key.as_ref().ok_or_else(|| MoomooError::Crypto {
                    message: "encryption requested but RSA private key is missing".to_string(),
                })?;
                rsa_decrypt(ciphertext, rsa_key)?
            } else {
                ciphertext.to_vec()
            }
        } else if state.encrypt {
            let key = state.aes_key.ok_or_else(|| MoomooError::Crypto {
                message: "AES key missing for encrypted session".to_string(),
            })?;
            let iv = state.aes_iv.ok_or_else(|| MoomooError::Crypto {
                message: "AES IV missing for encrypted session".to_string(),
            })?;
            aes_decrypt(&key, &iv, ciphertext)?
        } else {
            ciphertext.to_vec()
        };

        let calculated = sha1_digest(&plaintext);
        if calculated != header.sha1 {
            return Err(MoomooError::Protocol {
                message: format!("SHA1 mismatch for proto {}", header.proto_id),
            });
        }
        Ok(plaintext)
    }
}

pub async fn get_global_state(
    client: &MoomooClient,
) -> Result<get_global_state::Response, MoomooError> {
    let request = get_global_state::Request {
        c2s: get_global_state::C2s { user_id: 0 },
    };
    client.query(PROTO_ID_GET_GLOBAL_STATE, &request).await
}

fn encode_header(proto_id: u32, serial_no: u32, body_len: u32, sha1: [u8; 20]) -> [u8; HEADER_LEN] {
    let mut bytes = [0_u8; HEADER_LEN];
    bytes[0] = b'F';
    bytes[1] = b'T';
    bytes[2..6].copy_from_slice(&proto_id.to_le_bytes());
    bytes[6] = PROTO_FMT_PROTOBUF;
    bytes[7] = API_PROTO_VER;
    bytes[8..12].copy_from_slice(&serial_no.to_le_bytes());
    bytes[12..16].copy_from_slice(&body_len.to_le_bytes());
    bytes[16..36].copy_from_slice(&sha1);
    bytes
}

fn decode_header(bytes: &[u8; HEADER_LEN]) -> Result<PacketHeader, MoomooError> {
    if bytes[0] != b'F' || bytes[1] != b'T' {
        return Err(MoomooError::Protocol {
            message: "invalid packet header flag".to_string(),
        });
    }

    let mut sha1 = [0_u8; 20];
    sha1.copy_from_slice(&bytes[16..36]);

    Ok(PacketHeader {
        proto_id: u32::from_le_bytes(bytes[2..6].try_into().expect("slice length")),
        proto_fmt: bytes[6],
        proto_ver: bytes[7],
        serial_no: u32::from_le_bytes(bytes[8..12].try_into().expect("slice length")),
        body_len: u32::from_le_bytes(bytes[12..16].try_into().expect("slice length")),
        sha1,
    })
}

fn sha1_digest(bytes: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn rsa_encrypt(plaintext: &[u8], private_key: &RsaPrivateKey) -> Result<Vec<u8>, MoomooError> {
    let public_key = RsaPublicKey::from(private_key);
    let max_chunk = public_key.size().saturating_sub(11).min(100);
    let mut output = Vec::new();
    for chunk in plaintext.chunks(max_chunk) {
        let encrypted = public_key
            .encrypt(&mut OsRng, Pkcs1v15Encrypt, chunk)
            .map_err(|error| MoomooError::Crypto {
                message: format!("RSA encryption failed: {error}"),
            })?;
        output.extend_from_slice(&encrypted);
    }
    Ok(output)
}

fn rsa_decrypt(ciphertext: &[u8], private_key: &RsaPrivateKey) -> Result<Vec<u8>, MoomooError> {
    let chunk_size = private_key.size();
    if chunk_size == 0 || !ciphertext.len().is_multiple_of(chunk_size) {
        return Err(MoomooError::Crypto {
            message: "invalid RSA ciphertext length".to_string(),
        });
    }

    let mut output = Vec::new();
    for chunk in ciphertext.chunks(chunk_size) {
        let decrypted = private_key
            .decrypt(Pkcs1v15Encrypt, chunk)
            .map_err(|error| MoomooError::Crypto {
                message: format!("RSA decryption failed: {error}"),
            })?;
        output.extend_from_slice(&decrypted);
    }
    Ok(output)
}

fn aes_encrypt(key: &[u8; 16], iv: &[u8; 16], plaintext: &[u8]) -> Result<Vec<u8>, MoomooError> {
    let cipher = Aes128CbcEnc::new_from_slices(key, iv).map_err(|error| MoomooError::Crypto {
        message: format!("failed to initialize AES encryptor: {error}"),
    })?;
    Ok(cipher.encrypt_padded_vec_mut::<Pkcs7>(plaintext))
}

fn aes_decrypt(key: &[u8; 16], iv: &[u8; 16], ciphertext: &[u8]) -> Result<Vec<u8>, MoomooError> {
    let cipher = Aes128CbcDec::new_from_slices(key, iv).map_err(|error| MoomooError::Crypto {
        message: format!("failed to initialize AES decryptor: {error}"),
    })?;
    cipher
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|error| MoomooError::Crypto {
            message: format!("AES decryption failed: {error}"),
        })
}

fn parse_fixed_16(value: &str, field: &str) -> Result<[u8; 16], MoomooError> {
    let bytes = value.as_bytes();
    if bytes.len() != 16 {
        return Err(MoomooError::Protocol {
            message: format!("{field} must be 16 bytes, got {}", bytes.len()),
        });
    }

    let mut output = [0_u8; 16];
    output.copy_from_slice(bytes);
    Ok(output)
}

fn now_utc_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

impl fmt::Display for PacketHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PacketHeader(proto_id={}, proto_fmt={}, proto_ver={}, serial_no={}, body_len={})",
            self.proto_id, self.proto_fmt, self.proto_ver, self.serial_no, self.body_len
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let sha1 = [7_u8; 20];
        let encoded = encode_header(3203, 12345, 678, sha1);
        let decoded = decode_header(&encoded).expect("header should decode");
        assert_eq!(decoded.proto_id, 3203);
        assert_eq!(decoded.serial_no, 12345);
        assert_eq!(decoded.body_len, 678);
        assert_eq!(decoded.sha1, sha1);
    }

    #[test]
    fn parses_fixed_16() {
        let parsed = parse_fixed_16("0123456789abcdef", "field").expect("valid 16-byte string");
        assert_eq!(&parsed, b"0123456789abcdef");
    }
}
