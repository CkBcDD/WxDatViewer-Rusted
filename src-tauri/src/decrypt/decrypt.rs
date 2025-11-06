//! 微信 DAT 文件解密模块
//!
//! 该模块提供了解密微信 DAT 格式文件的功能,支持 v3 和 v4 两个版本。
//! - v3: 使用简单的 XOR 加密
//! - v4: 使用 AES-ECB + XOR 混合加密

use std::path::Path;

mod aes;
mod error;
mod v3;
mod v4;
mod version;

// 重新导出公共类型
pub use aes::AesHandler;
pub use error::DecryptError;
pub use v3::V3Decryptor;
pub use v4::{V4Decryptor, V4Header};
pub use version::{DatVersion, VersionDetector};

/// DAT 文件解密器
pub struct DatDecryptor;

impl DatDecryptor {
    /// 检测 DAT 文件版本
    ///
    /// # 参数
    ///
    /// * `input_path` - 输入文件路径
    ///
    /// # 返回
    ///
    /// 返回检测到的 DAT 版本
    pub fn detect_version<P: AsRef<Path>>(input_path: P) -> Result<DatVersion, DecryptError> {
        VersionDetector::detect(input_path)
    }

    /// 解密 v3 版本的 DAT 文件
    ///
    /// # 参数
    ///
    /// * `input_path` - 输入文件路径
    /// * `xor_key` - XOR 密钥
    ///
    /// # 返回
    ///
    /// 解密后的字节数据
    pub fn decrypt_dat_v3<P: AsRef<Path>>(
        input_path: P,
        xor_key: u8,
    ) -> Result<Vec<u8>, DecryptError> {
        V3Decryptor::decrypt(input_path, xor_key)
    }

    /// 解密 v4 版本的 DAT 文件
    ///
    /// # 参数
    ///
    /// * `input_path` - 输入文件路径
    /// * `xor_key` - XOR 密钥
    /// * `aes_key` - AES 密钥 (16 字节)
    ///
    /// # 返回
    ///
    /// 解密后的字节数据
    pub fn decrypt_dat_v4<P: AsRef<Path>>(
        input_path: P,
        xor_key: u8,
        aes_key: &[u8],
    ) -> Result<Vec<u8>, DecryptError> {
        V4Decryptor::decrypt(input_path, xor_key, aes_key)
    }

    /// 自动检测版本并解密 DAT 文件
    ///
    /// # 参数
    ///
    /// * `input_path` - 输入文件路径
    /// * `xor_key` - XOR 密钥
    /// * `aes_key` - AES 密钥 (仅 v4 版本需要)
    ///
    /// # 返回
    ///
    /// 解密后的字节数据
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dat_version_detection() {
        // 测试版本检测逻辑
        assert_eq!(VersionDetector::V4_V1_SIGNATURE, b"\x07\x08V1\x08\x07");
        assert_eq!(VersionDetector::V4_V2_SIGNATURE, b"\x07\x08V2\x08\x07");
    }

    #[test]
    fn test_pkcs7_unpad() {
        let mut data = vec![1, 2, 3, 4, 4, 4, 4]; // 4 bytes padding
        AesHandler::pkcs7_unpad(&mut data).unwrap();
        assert_eq!(data, vec![1, 2, 3]);

        let mut data = vec![1, 2, 3, 1]; // 1 byte padding
        AesHandler::pkcs7_unpad(&mut data).unwrap();
        assert_eq!(data, vec![1, 2, 3]);
    }

    #[test]
    fn test_xor_encryption() {
        let data = vec![0x01, 0x02, 0x03, 0x04];
        let key = 0xFF;
        let encrypted: Vec<u8> = data.iter().map(|&b| b ^ key).collect();
        let decrypted: Vec<u8> = encrypted.iter().map(|&b| b ^ key).collect();
        assert_eq!(data, decrypted);
    }
}
