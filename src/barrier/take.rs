use embedded_io_async::ErrorType;
use embedded_io_async::Read;

/**
 * A wrapper around a `Read` that limits the number of bytes that can be read.
 * Modified from std::io::Take as it needs to move the inner reader.
 */
pub struct Take<'a, T> {
    inner: &'a mut T,
    limit: u64,
}

impl<'a, T: Read> Take<'a, T> {
    pub fn new(inner: &'a mut T, limit: u64) -> Take<'a, T> {
        Take { inner, limit }
    }

    pub fn limit(&self) -> u64 {
        self.limit
    }

    pub async fn discard_all(&mut self) -> Result<(), T::Error> {
        let mut buf = [0; 1024];
        while self.limit > 0 {
            let n = self.read(&mut buf).await?;
            if n == 0 {
                break;
            }
        }
        Ok(())
    }
}

impl<'a, T: Read> ErrorType for Take<'a, T> {
    type Error = T::Error;
}

impl<'a, T: Read> Read for Take<'a, T>
where
    Take<'a, T>: ErrorType<Error = T::Error>,
{
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, <Take<'a, T> as ErrorType>::Error> {
        // Don't call into inner reader at all at EOF because it may still block
        if self.limit == 0 {
            return Ok(0);
        }

        let max = core::cmp::min(buf.len() as u64, self.limit) as usize;
        let n = self.inner.read(&mut buf[..max]).await?;
        assert!(n as u64 <= self.limit, "number of read bytes exceeds limit");
        self.limit -= n as u64;
        Ok(n)
    }
}
