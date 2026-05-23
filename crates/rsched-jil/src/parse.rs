//! Hand-written JIL line parser.
//!
//! JIL is line-oriented with `key: value` pairs.  A new block begins when a
//! `insert_job`, `update_job`, or `delete_job` verb appears.  `/* … */` block
//! comments (which Autosys uses as separators) are stripped first.

use crate::{
    error::JilError,
    spec::{JilJobType, JobSpec, PartialJobSpec},
    JilBlock,
};

/// Parse JIL text into a list of [`JilBlock`]s.
///
/// Warnings about unknown attributes are embedded in each block's `warnings`
/// field rather than being returned as errors.
pub fn parse(input: &str) -> Result<Vec<JilBlock>, JilError> {
    let stripped = strip_comments(input)?;
    let lines: Vec<&str> = stripped.lines().collect();
    let mut blocks: Vec<JilBlock> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() {
            i += 1;
            continue;
        }

        // Try to parse a verb from the line.
        if let Some(block) = try_parse_verb_line(line, &lines, &mut i)? {
            blocks.push(block);
        } else {
            i += 1;
        }
    }

    Ok(blocks)
}

/// Strip `/* ... */` block comments (may span multiple lines).
fn strip_comments(input: &str) -> Result<String, JilError> {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next(); // consume '*'
                          // scan until '*/'
            let mut found_end = false;
            while let Some(c2) = chars.next() {
                if c2 == '*' && chars.peek() == Some(&'/') {
                    chars.next(); // consume '/'
                    found_end = true;
                    break;
                }
            }
            if !found_end {
                return Err(JilError::UnterminatedComment);
            }
            // Replace the comment with a newline so line counting stays sane.
            out.push('\n');
        } else {
            out.push(c);
        }
    }

    Ok(out)
}

/// Attempt to detect and parse a verb line.  Returns `None` if the line is not
/// a verb line (the caller should advance and skip).  Advances `i` past all
/// consumed lines.
fn try_parse_verb_line(
    first_line: &str,
    lines: &[&str],
    i: &mut usize,
) -> Result<Option<JilBlock>, JilError> {
    // A verb line looks like: `insert_job: name   job_type: c`
    // or just: `delete_job: name`
    let (verb, rest) = match split_first_kv(first_line) {
        Some(pair) => pair,
        None => return Ok(None),
    };

    let verb_lc = verb.to_ascii_lowercase();
    let line_number = *i + 1;

    match verb_lc.as_str() {
        "insert_job" => {
            let (name, extra) = parse_name_and_rest(rest);
            let mut attrs = collect_inline_attrs(extra);
            *i += 1;
            gather_attrs(lines, i, &mut attrs);
            let spec = build_job_spec(name, attrs, line_number)?;
            Ok(Some(JilBlock::Insert(spec)))
        }
        "update_job" => {
            let (name, extra) = parse_name_and_rest(rest);
            let mut attrs = collect_inline_attrs(extra);
            *i += 1;
            gather_attrs(lines, i, &mut attrs);
            let partial = build_partial_spec(attrs);
            Ok(Some(JilBlock::Update(name.to_string(), partial)))
        }
        "delete_job" => {
            let (name, _) = parse_name_and_rest(rest);
            *i += 1;
            Ok(Some(JilBlock::Delete(name.to_string())))
        }
        other => {
            // Check whether this could be an attribute (no capital, contains
            // known keywords).  If it really looks like a verb, error out.
            if looks_like_verb(other) {
                Err(JilError::UnknownVerb(verb.to_string(), line_number))
            } else {
                Ok(None)
            }
        }
    }
}

/// Returns true if the token looks like it was intended as a JIL verb.
fn looks_like_verb(s: &str) -> bool {
    s.ends_with("_job")
}

/// Split `key: value` returning `(key, value_str)` or `None`.
fn split_first_kv(line: &str) -> Option<(&str, &str)> {
    let idx = line.find(':')?;
    Some((line[..idx].trim(), line[idx + 1..].trim()))
}

/// From the remainder of a verb line (`name   job_type: c`), extract the job
/// name and any inline `key: value` pairs that follow.
fn parse_name_and_rest(rest: &str) -> (&str, &str) {
    // The name is the first whitespace-delimited token before any colon that
    // is preceded by a key.
    let rest = rest.trim();
    // Find where inline attributes start.  The pattern is: name<ws>key: value.
    // We look for the first colon that is preceded by a non-space token.
    let name_end = find_name_end(rest);
    (&rest[..name_end], rest[name_end..].trim())
}

fn find_name_end(rest: &str) -> usize {
    let bytes = rest.as_bytes();
    // Walk past the name token (no whitespace), then look for `<ws>key:`.
    let mut i = 0;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Collect `key: value` pairs that appear *inline* on the verb line after the
/// job name.  (Autosys puts `job_type` inline with `insert_job`.)
fn collect_inline_attrs(s: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    // Multiple kv pairs separated by whitespace, format: key: value key2: val2
    // We scan for tokens followed by `:`
    let mut tokens = s.split_whitespace().peekable();
    while let Some(tok) = tokens.next() {
        if let Some(key) = tok.strip_suffix(':') {
            // value is the next token
            if let Some(value) = tokens.next() {
                attrs.push((key.to_ascii_lowercase(), value.to_string()));
            }
        } else if tok.contains(':') {
            // key:value without space
            if let Some((k, v)) = tok.split_once(':') {
                if !v.is_empty() {
                    attrs.push((k.to_ascii_lowercase(), v.to_string()));
                } else if let Some(value) = tokens.next() {
                    attrs.push((k.to_ascii_lowercase(), value.to_string()));
                }
            }
        }
    }
    attrs
}

/// Advance `i` consuming continuation lines (indented or plain `key: value`)
/// until we hit a blank line or a new verb line.
fn gather_attrs(lines: &[&str], i: &mut usize, attrs: &mut Vec<(String, String)>) {
    while *i < lines.len() {
        let line = lines[*i].trim();
        if line.is_empty() {
            *i += 1;
            continue;
        }
        // Stop if we see a new verb.
        if is_verb_line(line) {
            break;
        }
        // Parse `key: value` (value may contain spaces).
        if let Some(colon) = line.find(':') {
            let key = line[..colon].trim().to_ascii_lowercase();
            let value = unescape_value(line[colon + 1..].trim());
            attrs.push((key, value));
        }
        *i += 1;
    }
}

/// Returns true if the line starts with a known verb.
fn is_verb_line(line: &str) -> bool {
    let low = line.to_ascii_lowercase();
    low.starts_with("insert_job") || low.starts_with("update_job") || low.starts_with("delete_job")
}

/// Strip surrounding double quotes and unescape `\"`.
fn unescape_value(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        s[1..s.len() - 1].replace("\\\"", "\"")
    } else {
        s.to_string()
    }
}

/// Build a [`JobSpec`] from a collected attribute list.
fn build_job_spec(
    name: &str,
    attrs: Vec<(String, String)>,
    _line: usize,
) -> Result<JobSpec, JilError> {
    let mut spec = JobSpec::empty(name, JilJobType::Command);

    for (key, value) in attrs {
        apply_attr_to_spec(&mut spec, &key, &value)?;
    }

    Ok(spec)
}

fn apply_attr_to_spec(spec: &mut JobSpec, key: &str, value: &str) -> Result<(), JilError> {
    match key {
        "job_type" => {
            spec.job_type = parse_job_type(value, &spec.name)?;
        }
        "command" => spec.command = Some(value.to_string()),
        "machine" => spec.machine = Some(value.to_string()),
        "owner" => spec.owner = Some(value.to_string()),
        "days_of_week" => spec.days_of_week = Some(value.to_string()),
        "start_times" => spec.start_times = Some(value.to_string()),
        "condition" => spec.condition = Some(value.to_string()),
        "alarm_if_fail" => spec.alarm_if_fail = parse_yn(value, key)?,
        "n_retrys" | "n_retries" => {
            spec.n_retrys = value.parse::<u32>().map_err(|_| JilError::BadValue {
                attr: key.to_string(),
                detail: format!("{value:?} is not a non-negative integer"),
            })?;
        }
        "term_run_time" => {
            spec.term_run_time = Some(value.parse::<u64>().map_err(|_| JilError::BadValue {
                attr: key.to_string(),
                detail: format!("{value:?} is not a non-negative integer"),
            })?);
        }
        "description" => spec.description = Some(value.to_string()),
        "std_out_file" => spec.std_out_file = Some(value.to_string()),
        "std_err_file" => spec.std_err_file = Some(value.to_string()),
        "box_name" => spec.box_name = Some(value.to_string()),
        "exclude_calendar" => spec.exclude_calendar = Some(value.to_string()),
        "must_start_times" => spec.must_start_times = Some(value.to_string()),
        "must_complete_times" => spec.must_complete_times = Some(value.to_string()),
        "fail_codes" => spec.fail_codes = Some(value.to_string()),
        "max_exit_success" => {
            spec.max_exit_success = Some(value.parse::<i32>().map_err(|_| JilError::BadValue {
                attr: key.to_string(),
                detail: format!("{value:?} is not an integer"),
            })?);
        }
        "condition_code" => {
            spec.condition_code = Some(value.parse::<i32>().map_err(|_| JilError::BadValue {
                attr: key.to_string(),
                detail: format!("{value:?} is not an integer"),
            })?);
        }
        "box_success" => spec.box_success = Some(value.to_string()),
        "box_failure" => spec.box_failure = Some(value.to_string()),
        "box_terminator" => spec.box_terminator = Some(parse_yn(value, key)?),
        "job_terminator" => spec.job_terminator = Some(parse_yn(value, key)?),
        "auto_hold" => spec.auto_hold = Some(parse_yn(value, key)?),
        // Known-but-ignored attributes that exist in real JIL.
        "date_conditions"
        | "timezone"
        | "run_calendar"
        | "max_run_alarm"
        | "watch_file"
        | "watch_interval"
        | "watch_file_min_size"
        | "profile"
        | "application"
        | "group"
        | "permission" => {
            spec.warnings.push(format!(
                "attribute {key:?} is not yet mapped; value {value:?} ignored"
            ));
        }
        other => {
            spec.warnings
                .push(format!("unknown attribute {other:?} ignored"));
        }
    }
    Ok(())
}

fn build_partial_spec(attrs: Vec<(String, String)>) -> PartialJobSpec {
    let mut spec = PartialJobSpec::default();
    for (key, value) in attrs {
        apply_attr_to_partial(&mut spec, &key, &value);
    }
    spec
}

fn apply_attr_to_partial(spec: &mut PartialJobSpec, key: &str, value: &str) {
    match key {
        "command" => spec.command = Some(value.to_string()),
        "machine" => spec.machine = Some(value.to_string()),
        "owner" => spec.owner = Some(value.to_string()),
        "days_of_week" => spec.days_of_week = Some(value.to_string()),
        "start_times" => spec.start_times = Some(value.to_string()),
        "condition" => spec.condition = Some(value.to_string()),
        "alarm_if_fail" => {
            if let Ok(b) = parse_yn(value, key) {
                spec.alarm_if_fail = Some(b);
            }
        }
        "n_retrys" | "n_retries" => {
            if let Ok(n) = value.parse::<u32>() {
                spec.n_retrys = Some(n);
            }
        }
        "term_run_time" => {
            if let Ok(n) = value.parse::<u64>() {
                spec.term_run_time = Some(n);
            }
        }
        "description" => spec.description = Some(value.to_string()),
        "std_out_file" => spec.std_out_file = Some(value.to_string()),
        "std_err_file" => spec.std_err_file = Some(value.to_string()),
        "box_name" => spec.box_name = Some(value.to_string()),
        "exclude_calendar" => spec.exclude_calendar = Some(value.to_string()),
        "must_start_times" => spec.must_start_times = Some(value.to_string()),
        "must_complete_times" => spec.must_complete_times = Some(value.to_string()),
        "fail_codes" => spec.fail_codes = Some(value.to_string()),
        "max_exit_success" => {
            if let Ok(n) = value.parse::<i32>() {
                spec.max_exit_success = Some(n);
            }
        }
        "condition_code" => {
            if let Ok(n) = value.parse::<i32>() {
                spec.condition_code = Some(n);
            }
        }
        "box_success" => spec.box_success = Some(value.to_string()),
        "box_failure" => spec.box_failure = Some(value.to_string()),
        "box_terminator" => {
            if let Ok(b) = parse_yn(value, key) {
                spec.box_terminator = Some(b);
            }
        }
        "job_terminator" => {
            if let Ok(b) = parse_yn(value, key) {
                spec.job_terminator = Some(b);
            }
        }
        "auto_hold" => {
            if let Ok(b) = parse_yn(value, key) {
                spec.auto_hold = Some(b);
            }
        }
        other => {
            spec.warnings
                .push(format!("unknown attribute {other:?} ignored"));
        }
    }
}

fn parse_job_type(s: &str, job_name: &str) -> Result<JilJobType, JilError> {
    match s.to_ascii_lowercase().as_str() {
        "c" | "cmd" | "command" => Ok(JilJobType::Command),
        "box" => Ok(JilJobType::Box),
        "fw" | "file_watcher" => Ok(JilJobType::FileWatcher),
        other => Err(JilError::UnknownJobType(
            other.to_string(),
            job_name.to_string(),
        )),
    }
}

fn parse_yn(s: &str, attr: &str) -> Result<bool, JilError> {
    match s.to_ascii_lowercase().trim() {
        "y" | "yes" | "true" | "1" => Ok(true),
        "n" | "no" | "false" | "0" => Ok(false),
        other => Err(JilError::BadValue {
            attr: attr.to_string(),
            detail: format!("{other:?} is not y/n"),
        }),
    }
}
