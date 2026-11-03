use cadcraft_engine::Session;
use serde_json::json;

use super::args::{ReviewArgs, ReviewQuality};
use super::error::Result;
use super::output::Streams;
use super::{open, unicode};

pub(crate) fn review(args: &ReviewArgs, io: &mut Streams) -> Result {
    let mut session = Session::empty();
    open(&mut session, &args.input)?;
    let mut params = json!({
        "path": unicode(&args.output)?,
        "quality": match args.quality { ReviewQuality::Preview => "preview", ReviewQuality::Full => "8k" },
    });
    if let Some(directory) = &args.font_dir {
        params["fontDirectory"] = json!(unicode(directory)?);
    }
    let result = session.execute("reviewplot", &params)?;
    io.outln(format_args!("{result}"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::path::PathBuf;

    #[test]
    fn compiled_review_matches_explicit_plot_and_keeps_source_and_session_unchanged() {
        crate::install_io();
        let directory = std::env::temp_dir().join(format!("cadcraft-compiled-review-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let input = directory.join("original.dxf");
        let expected = directory.join("explicit.png");
        let actual = directory.join("compiled.png");
        let drawing = cadcraft_engine::sample::default_sample();
        let original = cadcraft_io::write(&drawing, "original.dxf").unwrap();
        std::fs::write(&input, &original).unwrap();
        let mut session = Session::empty();
        open(&mut session, &input).unwrap();
        let before = session.doc().unwrap().clone();
        let result = session.execute("reviewplot", &json!({"quality":"preview"})).unwrap();
        assert_eq!(session.doc().unwrap(), &before);
        assert_eq!(result["profile"], "mechanical-review-v1");
        session.execute("setvar", &json!({"name":"LWDEFAULT","value":70})).unwrap();
        session.execute("plot", &json!({"path":expected,"layout":"Model","output":"png","width":365.76,"height":650.24,"margins":[0,0,0,0],"resolution":100,"landscape":true,"fit":true,"center":true,"lineweights":true,"plotArea":"extents","plotStyleTable":"monochrome.ctb"})).unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert_eq!(
            crate::run(["review", input.to_str().unwrap(), actual.to_str().unwrap()], &mut stdout, &mut stderr),
            0,
            "{}",
            String::from_utf8_lossy(&stderr)
        );
        let generated = std::fs::read(&actual).unwrap();
        assert_eq!(generated, std::fs::read(&expected).unwrap());
        assert_eq!(&generated[16..24], &[0, 0, 10, 0, 0, 0, 5, 160]); // 2560×1440 PNG header
        assert_eq!(std::fs::read(&input).unwrap(), original);
        let receipt: Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(receipt["quality"], "preview");
        assert_eq!(receipt["sourceDrawingModified"], false);
        assert_eq!(receipt["sha256"].as_str().unwrap().len(), 64);
        assert_eq!(crate::run(["review", input.to_str().unwrap(), actual.to_str().unwrap()], &mut Vec::new(), &mut Vec::new()), 1);
        assert_eq!(std::fs::read(&actual).unwrap(), generated);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn review_cli_rejects_quality_typos_unknown_overrides_and_input_aliases() {
        for args in [vec!["review", "a.dxf", "b.png", "--quality", "draft"], vec!["review", "a.dxf", "b.png", "--resolution", "200"]] {
            assert_eq!(crate::run(args, &mut Vec::new(), &mut Vec::new()), 2);
        }
        let path: PathBuf = std::env::temp_dir().join(format!("cadcraft-review-alias-{}.png", std::process::id()));
        std::fs::write(&path, b"original source bytes").unwrap();
        assert_eq!(crate::run(["review", path.to_str().unwrap(), path.to_str().unwrap()], &mut Vec::new(), &mut Vec::new()), 2);
        assert_eq!(std::fs::read(&path).unwrap(), b"original source bytes");
        let _ = std::fs::remove_file(path);
    }
}
