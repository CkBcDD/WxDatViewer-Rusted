//! DAT 解密错误类型

use thiserror::Error;

/// DAT 解密错误类型
#[derive(Error, Debug)]
pub enum DecryptError {
    #[error("文件读取失败: {0}")]
    IoError(#[from] std::io::Error),

    #[error("文件格式无效")]
    #[allow(dead_code)] // 保留以供未来使用
    InvalidFormat,

    #[error("AES 解密失败: {0}")]
    AesDecryptError(String),

    #[error("不支持的 DAT 版本")]
    UnsupportedVersion,

    #[error("文件头解析失败")]
    HeaderParseError,
}
