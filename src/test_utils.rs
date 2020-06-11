use crate::{
    adapters::{AdaptInfo, ReadBox},
    args::RgaConfig,
    matching::{FastMatcher, SlowMatcher},
    preproc::PreprocConfig,
};
use std::{
    path::{Path, PathBuf},
};

pub fn test_data_dir() -> PathBuf {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push("exampledir/test/");
    d
}

pub fn simple_adapt_info(filepath: &Path, inp: ReadBox) -> (AdaptInfo, SlowMatcher) {
    (
        AdaptInfo {
            filepath_hint: filepath.to_owned(),
            is_real_file: true,
            archive_recursion_depth: 0,
            inp,
            line_prefix: "PREFIX:".to_string(),
            config: PreprocConfig {
                cache: None,
                args: RgaConfig::default(),
            },
        },
        FastMatcher::FileExtension(filepath.extension().unwrap().to_string_lossy().into_owned())
            .into(),
    )
}
