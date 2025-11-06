//! DAT 解密错误类型

use crate::error::AppError;

/// DAT 解密错误类型
#[derive(Debug, Clone)]
pub enum DecryptError {
    IoError(String),
    InvalidFormat,
    AesDecryptError(String),
    UnsupportedVersion,
    HeaderParseError,
}

impl From<DecryptError> for AppError {
    fn from(err: DecryptError) -> Self {
        match err {
            DecryptError::IoError(msg) => AppError::Internal(format!("文件读取失败: {}", msg)),
            DecryptError::InvalidFormat => AppError::InvalidDatFormat,
            DecryptError::AesDecryptError(msg) => AppError::AesDecryptError(msg),
            DecryptError::UnsupportedVersion => AppError::UnsupportedDatVersion,
            DecryptError::HeaderParseError => AppError::DatHeaderParseError,
        }
    }
}

impl From<std::io::Error> for DecryptError {
    fn from(err: std::io::Error) -> Self {
        DecryptError::IoError(err.to_string())
    }
}
