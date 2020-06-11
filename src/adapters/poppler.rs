




/*
static EXTENSIONS: &[&str] = &["pdf"];


    postproc: "add_lines"
    fn postproc(line_prefix: &str, inp: &mut dyn Read, oup: &mut dyn Write) -> Result<()> {
        // prepend Page X to each line
        let mut page = 1;
        for line in BufReader::new(inp).lines() {
            let mut line = line?;
            if line.contains('\x0c') {
                // page break
                line = line.replace('\x0c', "");
                page += 1;
                if line.is_empty() {
                    continue;
                }
            }
            oup.write_all(format!("{}Page {}: {}\n", line_prefix, page, line).as_bytes())?;
        }
        Ok(())
    }
}
*/
