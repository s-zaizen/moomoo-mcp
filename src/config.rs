use std::{env, path::PathBuf, time::Duration};

use crate::opend::MoomooError;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 11111;
const DEFAULT_CLIENT_VER: i32 = 300;
const DEFAULT_CLIENT_ID: &str = "moomoo-mcp";

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub opend_telnet_host: String,
    pub opend_telnet_port: Option<u16>,
    pub opend_telnet_read_timeout: Duration,
    pub use_encryption: bool,
    pub recv_notify: bool,
    pub client_ver: i32,
    pub client_id: String,
    pub rsa_private_key_path: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> Result<Self, MoomooError> {
        let host = env::var("MOOMOO_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());

        Ok(Self {
            opend_telnet_host: env::var("MOOMOO_OPEND_TELNET_HOST")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| host.clone()),
            opend_telnet_port: parse_env_u16("MOOMOO_OPEND_TELNET_PORT")?,
            opend_telnet_read_timeout: Duration::from_millis(
                parse_env_u64("MOOMOO_OPEND_TELNET_TIMEOUT_MS")?.unwrap_or(500),
            ),
            host,
            port: parse_env_u16("MOOMOO_PORT")?.unwrap_or(DEFAULT_PORT),
            use_encryption: parse_env_bool("MOOMOO_USE_ENCRYPTION")?.unwrap_or(false),
            recv_notify: parse_env_bool("MOOMOO_RECV_NOTIFY")?.unwrap_or(false),
            client_ver: parse_env_i32("MOOMOO_CLIENT_VER")?.unwrap_or(DEFAULT_CLIENT_VER),
            client_id: env::var("MOOMOO_CLIENT_ID")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| format!("{DEFAULT_CLIENT_ID}-{}", std::process::id())),
            rsa_private_key_path: env::var("MOOMOO_RSA_PRIVATE_KEY_PATH")
                .ok()
                .map(PathBuf::from),
        })
    }

    pub fn rsa_private_key_pem(&self) -> Result<String, MoomooError> {
        if let Some(path) = &self.rsa_private_key_path {
            return std::fs::read_to_string(path).map_err(|error| MoomooError::Config {
                message: format!(
                    "failed to read MOOMOO_RSA_PRIVATE_KEY_PATH '{}': {error}",
                    path.display()
                ),
            });
        }

        Ok(include_str!("../vendor/moomoo/conn_key.pem").to_string())
    }
}

fn parse_env_bool(key: &str) -> Result<Option<bool>, MoomooError> {
    let Some(value) = env::var(key).ok() else {
        return Ok(None);
    };

    match normalize_bool(&value).as_str() {
        "1" | "TRUE" | "YES" | "ON" => Ok(Some(true)),
        "0" | "FALSE" | "NO" | "OFF" => Ok(Some(false)),
        _ => Err(MoomooError::Config {
            message: format!("invalid boolean value for {key}: {value}"),
        }),
    }
}

fn parse_env_u16(key: &str) -> Result<Option<u16>, MoomooError> {
    let Some(value) = env::var(key).ok() else {
        return Ok(None);
    };

    value
        .parse::<u16>()
        .map(Some)
        .map_err(|error| MoomooError::Config {
            message: format!("invalid u16 value for {key}: {value} ({error})"),
        })
}

fn parse_env_i32(key: &str) -> Result<Option<i32>, MoomooError> {
    let Some(value) = env::var(key).ok() else {
        return Ok(None);
    };

    value
        .parse::<i32>()
        .map(Some)
        .map_err(|error| MoomooError::Config {
            message: format!("invalid i32 value for {key}: {value} ({error})"),
        })
}

fn parse_env_u64(key: &str) -> Result<Option<u64>, MoomooError> {
    let Some(value) = env::var(key).ok() else {
        return Ok(None);
    };

    value
        .parse::<u64>()
        .map(Some)
        .map_err(|error| MoomooError::Config {
            message: format!("invalid u64 value for {key}: {value} ({error})"),
        })
}

fn normalize_bool(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}
