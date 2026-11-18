//! Placeholder expansion → argv (spec §6.2). Pure: no I/O.
//! Engines run with the job's temp dir as cwd, so {out} and {out_dir} are the
//! relative names "out.<ext>" and "out".
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::Preset;

pub struct BuildInput<'a> {
    /// Absolute originals, or relative staged names (in-1.png) for stage_inputs presets.
    pub inputs: &'a [PathBuf],
    /// Folder of the first original input (pandoc --resource-path).
    pub in_dir: &'a Path,
    pub out_ext: &'a str,
    /// setting.* plus job values: reader, final_stem, video_kbps.
    pub vars: &'a BTreeMap<String, String>,
}

const FFMPEG_PREFIX: &[&str] = &["-hide_banner", "-nostdin", "-progress", "pipe:1", "-nostats"];

pub fn expand(template: &str, values: &BTreeMap<String, String>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('}').ok_or_else(|| format!("unclosed placeholder in '{template}'"))?;
        let key = &after[..end];
        out.push_str(values.get(key).ok_or_else(|| format!("unknown placeholder {{{key}}}"))?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn path_str(p: &Path) -> Result<String, String> {
    let s = p.to_str().ok_or_else(|| format!("path is not valid Unicode: {}", p.display()))?;
    if s.starts_with('-') {
        return Err(format!("input path starts with '-': {s}"));
    }
    Ok(s.to_string())
}

pub fn build_command(preset: &Preset, b: &BuildInput) -> Result<Vec<Vec<OsString>>, String> {
    let first = b.inputs.first().ok_or("no input files")?;
    let mut values = b.vars.clone();
    values.insert("in".into(), path_str(first)?);
    values.insert("in_dir".into(), path_str(b.in_dir)?);
    values.insert("out".into(), format!("out.{}", b.out_ext));
    values.insert("out_dir".into(), "out".into());
    let inputs: Vec<String> = b.inputs.iter().map(|p| path_str(p)).collect::<Result<_, _>>()?;

    let mut passes = Vec::new();
    for template in &preset.passes {
        let mut argv: Vec<OsString> = Vec::new();
        if preset.engine == "ffmpeg" {
            argv.extend(FFMPEG_PREFIX.iter().map(OsString::from));
        }
        for arg in template {
            if arg.contains("{inputs}") {
                for input in &inputs {
                    let mut v = values.clone();
                    v.insert("inputs".into(), input.clone());
                    argv.push(expand(arg, &v)?.into());
                }
            } else {
                argv.push(expand(arg, &values)?.into());
            }
        }
        passes.push(argv);
    }
    Ok(passes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::{tests::MINI, Catalog};
    use std::path::PathBuf;

    fn vars() -> BTreeMap<String, String> {
        crate::settings::Settings::default().to_vars()
    }

    fn argv(v: &[OsString]) -> Vec<String> {
        v.iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn substitutes_and_prefixes_ffmpeg_flags() {
        let c = Catalog::parse(MINI).unwrap();
        let inputs = vec![PathBuf::from(r"C:\in\café 日本 %[1] -x.mov")];
        let v = vars();
        let passes = build_command(c.get("v-mp3").unwrap(), &BuildInput { inputs: &inputs, in_dir: Path::new(r"C:\in"), out_ext: "mp3", vars: &v }).unwrap();
        assert_eq!(
            argv(&passes[0]),
            ["-hide_banner", "-nostdin", "-progress", "pipe:1", "-nostats", "-i", r"C:\in\café 日本 %[1] -x.mov", "-b:a", "192k", "out.mp3"]
        );
    }

    #[test]
    fn inputs_expand_to_one_arg_each() {
        let c = Catalog::parse(MINI).unwrap();
        let inputs = vec![PathBuf::from(r"C:\in\a.pdf"), PathBuf::from(r"C:\in\b c.pdf")];
        let v = vars();
        let passes = build_command(c.get("pdf-merge").unwrap(), &BuildInput { inputs: &inputs, in_dir: Path::new(r"C:\in"), out_ext: "pdf", vars: &v }).unwrap();
        assert_eq!(argv(&passes[0]), ["--empty", "--pages", r"C:\in\a.pdf", r"C:\in\b c.pdf", "--", "out.pdf"]);
    }

    #[test]
    fn rejects_inputs_that_look_like_options() {
        let c = Catalog::parse(MINI).unwrap();
        let inputs = vec![PathBuf::from("-rf.mov")];
        let v = vars();
        let err = build_command(c.get("v-mp3").unwrap(), &BuildInput { inputs: &inputs, in_dir: Path::new(r"C:\in"), out_ext: "mp3", vars: &v }).unwrap_err();
        assert!(err.contains("starts with '-'"), "{err}");
    }

    #[test]
    fn unknown_placeholder_is_an_error() {
        let mut values = BTreeMap::new();
        values.insert("in".to_string(), "x".to_string());
        assert!(expand("{in}{nope}", &values).unwrap_err().contains("{nope}"));
        assert_eq!(expand("{in}[0]", &values).unwrap(), "x[0]");
    }
}
