use crate::preproc::rga_preproc;
use crate::{adapted_iter::AdaptedFilesIterBox, adapters::*};

use std::io::Read;

pub struct RecursingConcattyReader<'a> {
    inp: AdaptedFilesIterBox<'a>,
    cur: Option<ReadBox<'a>>,
}
impl<'a> RecursingConcattyReader<'a> {
    pub fn concat(inp: AdaptedFilesIterBox<'a>) -> anyhow::Result<Box<dyn Read + 'a>> {
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
impl<'a> Read for RecursingConcattyReader<'a> {
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
