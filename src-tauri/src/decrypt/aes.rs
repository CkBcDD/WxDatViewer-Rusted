//! AES 加密/解密模块

use super::error::DecryptError;

#[allow(deprecated)]
use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};
use aes::Aes128;

/// AES 加密处理器
pub struct AesHandler;

impl AesHandler {
    pub const BLOCK_SIZE: usize = 16;

    #[allow(deprecated)]
    pub fn decrypt_ecb(data: &[u8], key: &[u8]) -> Result<Vec<u8>, DecryptError> {
        if key.len() != 16 {
            return Err(DecryptError::AesDecryptError(
                "AES 密钥必须为 16 字节".to_string(),
            ));
        }

        // 使用 new_from_slice 避免直接构造 GenericArray 以提升兼容性
        let cipher = Aes128::new_from_slice(key)
            .map_err(|_| DecryptError::AesDecryptError("AES 密钥长度无效".to_string()))?;

        let mut result = data.to_vec();

        // 按块解密
        for chunk in result.chunks_exact_mut(Self::BLOCK_SIZE) {
            // 使用与 aes/crate 相同路径的 GenericArray，避免版本冲突
            let block = GenericArray::from_mut_slice(chunk);
            cipher.decrypt_block(block);
        }

        // 移除 PKCS7 填充
        Self::pkcs7_unpad(&mut result)?;

        Ok(result)
    }

    pub fn pkcs7_unpad(data: &mut Vec<u8>) -> Result<(), DecryptError> {
        if data.is_empty() {
            return Err(DecryptError::AesDecryptError("数据为空".to_string()));
        }

        let padding_len = *data.last().unwrap() as usize;

        if padding_len == 0 || padding_len > Self::BLOCK_SIZE || padding_len > data.len() {
            return Err(DecryptError::AesDecryptError("无效的填充".to_string()));
        }

        let start = data.len() - padding_len;
        if !data[start..].iter().all(|&b| b == padding_len as u8) {
            return Err(DecryptError::AesDecryptError("填充验证失败".to_string()));
        }

        data.truncate(start);
        Ok(())
    }

    pub fn align_size(size: usize) -> usize {
        size + (Self::BLOCK_SIZE - size % Self::BLOCK_SIZE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_align_size() {
        assert_eq!(AesHandler::align_size(15), 16);
        assert_eq!(AesHandler::align_size(16), 32);
        assert_eq!(AesHandler::align_size(17), 32);
    }
}
