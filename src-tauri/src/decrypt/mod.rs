//! 微信 DAT 文件解密模块
//!
//! 该模块提供了解密微信 DAT 格式文件的功能,支持 v3 和 v4 两个版本。

pub mod aes;
pub mod error;
pub mod v3;
pub mod v4;
pub mod version;

// 重新导出公共类型
pub use error::DecryptError;
pub use v3::V3Decryptor;
pub use v4::V4Decryptor;
pub use version::{DatVersion, VersionDetector};

use std::path::Path;

/// DAT 文件解密器
pub struct DatDecryptor;

impl DatDecryptor {
    /// 检测 DAT 文件版本
    pub fn detect_version<P: AsRef<Path>>(input_path: P) -> Result<DatVersion, DecryptError> {
        VersionDetector::detect(input_path)
    }

    /// 解密 v3 版本的 DAT 文件
    pub fn decrypt_dat_v3<P: AsRef<Path>>(
        input_path: P,
        xor_key: u8,
    ) -> Result<Vec<u8>, DecryptError> {
        V3Decryptor::decrypt(input_path, xor_key)
    }

    /// 解密 v4 版本的 DAT 文件
    pub fn decrypt_dat_v4<P: AsRef<Path>>(
        input_path: P,
        xor_key: u8,
        aes_key: &[u8],
    ) -> Result<Vec<u8>, DecryptError> {
        V4Decryptor::decrypt(input_path, xor_key, aes_key)
    }

    /// 自动检测版本并解密 DAT 文件
    pub fn decrypt<P: AsRef<Path>>(
        input_path: P,
        xor_key: u8,
        aes_key: Option<&[u8]>,
    ) -> Result<Vec<u8>, DecryptError> {
        let version = Self::detect_version(&input_path)?;

        match version {
            DatVersion::V3 => Self::decrypt_dat_v3(input_path, xor_key),
            DatVersion::V4V1 | DatVersion::V4V2 => {
                let key = aes_key.ok_or(DecryptError::AesDecryptError(
                    "v4 版本需要提供 AES 密钥".to_string(),
                ))?;
                Self::decrypt_dat_v4(input_path, xor_key, key)
            }
            DatVersion::Unknown => Err(DecryptError::UnsupportedVersion),
        }
    }
}
