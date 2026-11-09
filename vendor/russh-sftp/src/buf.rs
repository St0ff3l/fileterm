use bytes::Buf;
use encoding_rs::Encoding;

use crate::error::Error;

pub trait TryBuf: Buf {
    fn try_get_bytes(&mut self) -> Result<Vec<u8>, Error>;
    fn try_get_string_with_encoding(&mut self, encoding: &'static Encoding) -> Result<String, Error>;
}

impl<T: Buf> TryBuf for T {
    fn try_get_bytes(&mut self) -> Result<Vec<u8>, Error> {
        let len = self
            .try_get_u32()
            .map_err(|e| Error::UnexpectedBehavior(e.to_string()))? as usize;
        if self.remaining() < len {
            return Err(Error::BadMessage("no remaining for vec".to_owned()));
        }

        Ok(self.copy_to_bytes(len).to_vec())
    }

    fn try_get_string_with_encoding(&mut self, encoding: &'static Encoding) -> Result<String, Error> {
        let bytes = self.try_get_bytes()?;
        Ok(encoding.decode_without_bom_handling(&bytes).0.into_owned())
    }
}
