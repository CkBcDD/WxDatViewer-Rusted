//! DAT v3 版本解密模块

use super::error::DecryptError;
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// v3 版本解密器
pub struct V3Decryptor;

impl V3Decryptor {
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
    pub fn decrypt<P: AsRef<Path>>(input_path: P, xor_key: u8) -> Result<Vec<u8>, DecryptError> {
        let mut file = File::open(input_path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        log::debug!("解密 v3 DAT 文件,大小: {} 字节", data.len());

        // XOR 解密
        let decrypted = Self::xor_decrypt(&data, xor_key);

        log::debug!("v3 解密完成");

        Ok(decrypted)
    }

    /// XOR 解密
    ///
    /// # 参数
    ///
    /// * `data` - 待解密数据
    /// * `key` - XOR 密钥
    ///
    /// # 返回
    ///
    /// 解密后的数据
    pub fn xor_decrypt(data: &[u8], key: u8) -> Vec<u8> {
        data.iter().map(|&b| b ^ key).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xor_encryption() {
        let data = vec![0x01, 0x02, 0x03, 0x04];
        let key = 0xFF;
        let encrypted = V3Decryptor::xor_decrypt(&data, key);
        let decrypted = V3Decryptor::xor_decrypt(&encrypted, key);
        assert_eq!(data, decrypted);
    }
}
