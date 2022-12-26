use super::{FileAdapter, GetMetadata, ReadBox};
use anyhow::Result;
use tokio::io::AsyncWrite;
// use async_trait::async_trait;

pub trait WritingFileAdapter: GetMetadata + Send + Clone {
    fn adapt_write(
        &self,
        a: super::AdaptInfo,
        detection_reason: &crate::matching::FileMatcher,
        oup: &mut (dyn AsyncWrite),
    ) -> Result<()>;
}

/* struct PipedReadWriter {
    inner: ReadBox,
    pipe_thread: Thread,
}

impl<'a> Read for PipedReadWriter<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        todo!()
    }
}*/

impl FileAdapter for WritingFileAdapter {
    fn adapt(
        &self,
        ai_outer: super::AdaptInfo,
        detection_reason: &crate::matching::FileMatcher,
    ) -> anyhow::Result<ReadBox> {
        let (r, w) = crate::pipe::pipe();
        let cc = self.inner.clone();
        let detc = detection_reason.clone();
        panic!("ooo");
        // cc.adapt_write(ai_outer, detc, )
        /*tokio::spawn(move || {
            let mut oup = w;
            let ai = ai_outer;
            let res = cc.adapt_write(ai, &detc, &mut oup);
            if let Err(e) = res {
                oup.write_err(std::io::Error::new(std::io::ErrorKind::Other, e))
                    .expect("could not write err");
            }
        }); */

        //Ok(Box::new(r))
    }
}
