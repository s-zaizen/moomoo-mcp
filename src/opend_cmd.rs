use encoding_rs::GBK;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};

use crate::{config::Config, opend::MoomooError};

#[derive(Debug, Clone)]
pub struct OpenDCommandClient {
    config: Config,
}

#[derive(Debug, Clone)]
pub struct OperationReply {
    pub command: String,
    pub output: String,
}

impl OpenDCommandClient {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn relogin(&self, password_md5: Option<&str>) -> Result<OperationReply, MoomooError> {
        let params = password_md5
            .map(|value| vec![("login_pwd_md5", value.to_string())])
            .unwrap_or_default();
        self.execute("relogin", &params).await
    }

    pub async fn request_phone_verify_code(&self) -> Result<OperationReply, MoomooError> {
        self.execute("req_phone_verify_code", &[]).await
    }

    pub async fn submit_phone_verify_code(
        &self,
        code: &str,
    ) -> Result<OperationReply, MoomooError> {
        self.execute("input_phone_verify_code", &[("code", code.to_string())])
            .await
    }

    pub async fn request_picture_verify_code(&self) -> Result<OperationReply, MoomooError> {
        self.execute("req_pic_verify_code", &[]).await
    }

    pub async fn submit_picture_verify_code(
        &self,
        code: &str,
    ) -> Result<OperationReply, MoomooError> {
        self.execute("input_pic_verify_code", &[("code", code.to_string())])
            .await
    }

    async fn execute(
        &self,
        command: &str,
        params: &[(&str, String)],
    ) -> Result<OperationReply, MoomooError> {
        let port = self.config.opend_telnet_port.ok_or_else(|| MoomooError::Config {
            message: "OpenD Operation Command requires MOOMOO_OPEND_TELNET_PORT and an enabled Telnet port in OpenD".to_string(),
        })?;
        let mut stream = TcpStream::connect((self.config.opend_telnet_host.as_str(), port)).await?;
        let command_line = format_operation_command(command, params);
        stream.write_all(command_line.as_bytes()).await?;
        stream.write_all(b"\r\n").await?;
        stream.flush().await?;

        let mut reply = Vec::new();
        let mut buf = [0_u8; 4096];
        loop {
            match timeout(self.config.opend_telnet_read_timeout, stream.read(&mut buf)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(read)) => reply.extend_from_slice(&buf[..read]),
                Ok(Err(error)) => return Err(MoomooError::Io(error)),
                Err(_) => break,
            }
        }

        Ok(OperationReply {
            command: command.to_string(),
            output: decode_telnet_reply(&reply),
        })
    }
}

fn format_operation_command(command: &str, params: &[(&str, String)]) -> String {
    let mut line = command.to_string();
    for (key, value) in params {
        line.push(' ');
        line.push('-');
        line.push_str(key);
        line.push('=');
        line.push_str(value);
    }
    line
}

fn decode_telnet_reply(reply: &[u8]) -> String {
    let trimmed = trim_line_endings(reply);
    if trimmed.is_empty() {
        return String::new();
    }

    if let Ok(decoded) = String::from_utf8(trimmed.to_vec()) {
        return decoded;
    }

    let (decoded, _, _) = GBK.decode(trimmed);
    decoded.into_owned()
}

fn trim_line_endings(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && matches!(bytes[end - 1], b'\r' | b'\n') {
        end -= 1;
    }
    &bytes[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_operation_command() {
        let line =
            format_operation_command("input_phone_verify_code", &[("code", "123456".to_string())]);
        assert_eq!(line, "input_phone_verify_code -code=123456");
    }

    #[test]
    fn trims_line_endings_from_reply() {
        assert_eq!(decode_telnet_reply(b"ok\r\n"), "ok");
    }
}
