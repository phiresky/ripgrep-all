use super::{FileAdapter, GetMetadata, ReadBox};
use anyhow::Result;
use std::io::Read;
use std::io::Write;
use std::thread::Thread;

// this trait / struct split is ugly but necessary because of "conflicting trait implementation" otherwise with SpawningFileAdapter
#[dyn_clonable::clonable]
pub trait WritingFileAdapterTrait: GetMetadata + Send + Clone {
    fn adapt_write<'a>(
        &self,
        a: super::AdaptInfo<'a>,
        detection_reason: &crate::matching::FileMatcher,
        oup: &mut (dyn Write + 'a),
    ) -> Result<()>;
}

pub struct WritingFileAdapter {
    inner: Box<dyn WritingFileAdapterTrait>,
}
impl WritingFileAdapter {
    pub fn new(inner: Box<dyn WritingFileAdapterTrait>) -> WritingFileAdapter {
        WritingFileAdapter { inner }
    }
}

impl GetMetadata for WritingFileAdapter {
    fn metadata(&self) -> &super::AdapterMeta {
        self.inner.metadata()
    }
}

struct PipedReadWriter<'a> {
    inner: ReadBox<'a>,
    pipe_thread: Thread,
}

impl<'a> Read for PipedReadWriter<'a> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        todo!()
    }
}

impl FileAdapter for WritingFileAdapter {
    fn adapt<'a>(
        &self,
        ai_outer: super::AdaptInfo<'a>,
        detection_reason: &crate::matching::FileMatcher,
    ) -> anyhow::Result<ReadBox<'a>> {
        let (r, w) = crate::pipe::pipe();
        let cc = self.inner.clone();
        let detc = detection_reason.clone();
        std::thread::spawn(move || {
            let mut oup = w;
            let ai = ai_outer;
            let res = cc.adapt_write(ai, &detc, &mut oup);
            if let Err(e) = res {
                oup.write_err(std::io::Error::new(std::io::ErrorKind::Other, e))
                    .expect("could not write err");
            }
        });

        Ok(Box::new(r))
    }
}
