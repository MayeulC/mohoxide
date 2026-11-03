use lazy_regex::regex;
use miette::{Diagnostic, NamedSource, SourceSpan};
use std::sync::Arc;
use thiserror::Error;

/// A Lua error with source context, renderable by miette.
#[derive(Error, Diagnostic, Debug)]
#[error("{message}")]
pub struct LuaScriptError {
    pub message: String,

    #[source_code]
    pub src: NamedSource<Arc<String>>,

    #[label("here")]
    pub span: SourceSpan,

    // #[related]
    // pub frames: Vec<LuaFrame>,
    #[help]
    pub stack: Option<String>,
}

#[derive(Error, Diagnostic, Debug)]
#[error("  at {chunk}:{line}")]
pub struct LuaFrame {
    pub chunk: String,
    pub line: usize,

    #[source_code]
    pub src: NamedSource<Arc<String>>,

    #[label]
    pub span: SourceSpan,
}

fn span_for_line(source: &str, line: usize) -> SourceSpan {
    let mut offset = 0usize;
    for (idx, text) in source.split('\n').enumerate() {
        if idx + 1 == line {
            let content_len = text.trim_end_matches('\r').len();
            return SourceSpan::new(offset.into(), content_len);
        }
        offset += text.len() + 1; // '\r' included in text for CRLF; +1 for '\n'
    }
    SourceSpan::new(0.into(), 0)
}

pub fn prettify_lua_error(
    err: &mlua::Error,
    source: &str,
    _get_source: impl Fn(&str) -> Option<Arc<String>>,
) -> LuaScriptError {
    let (cause, _trace) = extract_error_parts(err);

    let re = regex!(r#"\[string "([^"]+)"\]:(\d+): (.*)"#);
    let (path, line, message) = if let Some(cap) = re.captures(&cause) {
        (
            cap[1].to_string(),
            cap[2].parse::<usize>().unwrap_or(0),
            cap[3].to_string(),
        )
    } else {
        ("unknown".to_string(), 0, cause.clone())
    };

    // println!("frame {} | {} | {}", path, line, message);
    // println!("cause {}", cause);
    // println!("trace {}", trace);

    // LuaScriptError {
    //     message,
    //     src: NamedSource::new(path, Arc::new(source.to_owned())),
    //     span: span_for_line(source, line),
    //     frames: Vec::new(),
    // }

    let stack = re
        .captures_iter(&cause)
        .skip(1)
        .map(|cap| {
            let path = &cap[1];
            let line: usize = cap[2].parse().unwrap();
            let message = &cap[3];

            // println!("frame {} | {} | {}", path, line, message);
            format!("  at {path}:{line} {message}")
            // let file = get_source(path).unwrap_or_default();
            // Some(LuaFrame {
            //     chunk: String::from(path),
            //     line: line,
            //     src: NamedSource::new(path, file),
            //     span: span_for_line(source, line),
            // })
        })
        .collect::<Vec<_>>()
        .join("\n");
    // let frames = re
    //     .captures_iter(&cause)
    //     .skip(1)
    //     .filter_map(|cap| {
    //         let path = &cap[1];
    //         let line: usize = cap[2].parse().unwrap();
    //         let message = &cap[3];

    //         println!("frame {} | {} | {}", path, line, message);

    //         let file = get_source(path).unwrap_or_default();
    //         Some(LuaFrame {
    //             chunk: String::from(path),
    //             line: line,
    //             src: NamedSource::new(path, file),
    //             span: span_for_line(source, line),
    //         })
    //     })
    //     .collect();

    LuaScriptError {
        message,
        src: NamedSource::new(path, Arc::new(source.to_owned())),
        span: span_for_line(source, line),
        stack: Some(stack), // frames: Vec::new(),
                            // frames,
    }
}

fn extract_error_parts(err: &mlua::Error) -> (String, String) {
    match err {
        mlua::Error::CallbackError { cause, traceback: _ } => extract_error_parts(cause),
        // mlua::Error::CallbackError { cause, traceback } => (cause.to_string(), traceback.clone()),
        mlua::Error::RuntimeError(s) => (s.clone(), String::new()),
        other => (other.to_string(), String::new()),
    }
}
