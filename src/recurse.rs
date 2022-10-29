use tokio_util::io::{ReaderStream, StreamReader};

use crate::{adapted_iter::AdaptedFilesIterBox, adapters::*};
use async_stream::stream;
use tokio_stream::StreamExt;

pub struct RecursingConcattyReader<'a> {
    inp: AdaptedFilesIterBox<'a>,
    cur: Option<ReadBox<'a>>,
}
pub fn concat_read_streams(
    mut input: AdaptedFilesIterBox<'_>,
) -> ReadBox<'_> {
    let s = stream! {
        while let Some(output) = input.next() {
            let mut stream = ReaderStream::new(output.inp);
            while let Some(bytes) = stream.next().await {
                yield bytes;
            }
        }
    };
    Box::pin(StreamReader::new(s))
}
/*
impl<'a> RecursingConcattyReader<'a> {
    pub fn concat(inp: AdaptedFilesIterBox<'a>) -> anyhow::Result<ReadBox<'a>> {
        let mut r = RecursingConcattyReader { inp, cur: None };
        r.ascend()?;
        Ok(Box::new(r))
    }
    pub fn ascend(&mut self) -> anyhow::Result<()> {
        let inp = &mut self.inp;
        // get next inner file from inp
        // we only need to access the inp: ReadIter when the inner reader is done, so this should be safe
        let ai = unsafe {
            // would love to make this safe, but how? something like OwnedRef<inp, cur>
            (*(inp as *mut AdaptedFilesIterBox<'a>)).next()
        };
        self.cur = match ai {
            Some(ai) => Some(rga_preproc(ai)?),
            None => None,
        };
        Ok(())
    }
}
impl<'a> AsyncRead for RecursingConcattyReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match &mut self.cur {
            None => Ok(0), // last file ended
            Some(cur) => match cur.read(buf) {
                Err(e) => Err(e),
                Ok(0) => {
                    // current file ended, go to next file
                    self.ascend()
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                    self.read(buf)
                }
                Ok(n) => Ok(n),
            },
        }
    }
}
*/