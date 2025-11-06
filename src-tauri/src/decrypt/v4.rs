//! DAT v4 版本解密模块

use super::aes::AesHandler;
use super::error::DecryptError;
use super::v3::V3Decryptor;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

/// v4 版本文件头结构
#[derive(Debug)]
pub struct V4Header {
    /// 签名 (6 字节)
    #[allow(dead_code)] // 保留签名字段用于调试和验证
    pub signature: [u8; 6],
    /// AES 加密部分大小
    pub aes_size: u32,
    /// XOR 加密部分大小
    pub xor_size: u32,
}

impl V4Header {
    /// 文件头大小
    pub const SIZE: usize = 15;

    /// 从字节数组解析文件头
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecryptError> {
        if bytes.len() < Self::SIZE {
            return Err(DecryptError::HeaderParseError);
        }

        let mut signature = [0u8; 6];
        signature.copy_from_slice(&bytes[0..6]);

        let aes_size = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]);
        let xor_size = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]);

        Ok(Self {
            signature,
            aes_size,
            xor_size,
        })
    }
}

/// v4 版本解密器
pub struct V4Decryptor;

impl V4Decryptor {
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
    pub fn decrypt<P: AsRef<Path>>(
        input_path: P,
        xor_key: u8,
        aes_key: &[u8],
    ) -> Result<Vec<u8>, DecryptError> {
        if aes_key.len() != 16 {
            return Err(DecryptError::AesDecryptError(
                "AES 密钥必须为 16 字节".to_string(),
            ));
        }

        let mut file = File::open(input_path)?;

        // 读取文件头
        let mut header_bytes = [0u8; V4Header::SIZE];
        file.read_exact(&mut header_bytes)?;
        let header = V4Header::from_bytes(&header_bytes)?;

        log::debug!(
            "解密 v4 DAT 文件,AES 大小: {}, XOR 大小: {}",
            header.aes_size,
            header.xor_size
        );

        // 解密 AES 部分
        let decrypted_aes = Self::decrypt_aes_section(&mut file, &header, aes_key)?;

        // 处理剩余数据
        let result = Self::decrypt_remaining_sections(&mut file, &header, xor_key, decrypted_aes)?;

        log::debug!("v4 解密完成,总大小: {} 字节", result.len());

        Ok(result)
    }

    /// 解密 AES 加密部分
    fn decrypt_aes_section(
        file: &mut File,
        header: &V4Header,
        aes_key: &[u8],
    ) -> Result<Vec<u8>, DecryptError> {
        // 计算 AES 对齐后的大小
        let aes_size_aligned = AesHandler::align_size(header.aes_size as usize);

        // 读取 AES 加密部分
        let mut aes_data = vec![0u8; aes_size_aligned];
        file.read_exact(&mut aes_data)?;

        // 解密 AES 部分
        AesHandler::decrypt_ecb(&aes_data, aes_key)
    }

    /// 解密剩余部分 (原始数据 + XOR 数据)
    fn decrypt_remaining_sections(
        file: &mut File,
        header: &V4Header,
        xor_key: u8,
        mut result: Vec<u8>,
    ) -> Result<Vec<u8>, DecryptError> {
        let xor_size = header.xor_size as usize;

        if xor_size > 0 {
            // 读取中间的原始数据
            let mut raw_data = Vec::new();
            let current_pos = file.stream_position()?;
            let file_len = file.metadata()?.len();
            let raw_len = file_len - current_pos - xor_size as u64;

            if raw_len > 0 {
                let mut buffer = vec![0u8; raw_len as usize];
                file.read_exact(&mut buffer)?;
                raw_data = buffer;
            }

            // 读取并解密 XOR 部分
            let mut xor_data = vec![0u8; xor_size];
            file.read_exact(&mut xor_data)?;
            let xored_data = V3Decryptor::xor_decrypt(&xor_data, xor_key);

            // 组合所有部分
            result.extend_from_slice(&raw_data);
            result.extend_from_slice(&xored_data);
        } else {
            // 没有 XOR 部分,读取剩余原始数据
            let mut raw_data = Vec::new();
            file.read_to_end(&mut raw_data)?;
            result.extend_from_slice(&raw_data);
        }

        Ok(result)
    }
}
