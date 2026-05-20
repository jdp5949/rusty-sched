//! Integration tests for the JIL parser.

use crate::{error::JilError, parse::parse, spec::JilJobType, JilBlock};

const SAMPLE_JIL: &str = r#"
insert_job: nightly_etl   job_type: c
command: /opt/etl/run.sh
machine: prod-etl-01
owner: etl@example.com
date_conditions: y
days_of_week: mo,tu,we,th,fr
start_times: "02:00"
description: "Nightly extract"
std_out_file: /var/log/etl.out
std_err_file: /var/log/etl.err
alarm_if_fail: y
n_retrys: 3
term_run_time: 60
condition: success(upstream_job) and notrunning(other_job)

/* ----------------- nightly_etl_box ----------------- */

insert_job: nightly_etl_box   job_type: box
description: "Box for nightly pipeline"

/* ----------------- update / delete ----------------- */

update_job: nightly_etl
alarm_if_fail: n

delete_job: old_job
"#;

#[test]
fn parse_sample_jil_no_error() {
    let blocks = parse(SAMPLE_JIL).unwrap();
    assert_eq!(blocks.len(), 4);
}

#[test]
fn insert_job_fields() {
    let blocks = parse(SAMPLE_JIL).unwrap();
    match &blocks[0] {
        JilBlock::Insert(spec) => {
            assert_eq!(spec.name, "nightly_etl");
            assert_eq!(spec.job_type, JilJobType::Command);
            assert_eq!(spec.command.as_deref(), Some("/opt/etl/run.sh"));
            assert_eq!(spec.machine.as_deref(), Some("prod-etl-01"));
            assert_eq!(spec.owner.as_deref(), Some("etl@example.com"));
            assert_eq!(spec.days_of_week.as_deref(), Some("mo,tu,we,th,fr"));
            assert_eq!(spec.start_times.as_deref(), Some("02:00"));
            assert_eq!(spec.description.as_deref(), Some("Nightly extract"));
            assert_eq!(spec.std_out_file.as_deref(), Some("/var/log/etl.out"));
            assert_eq!(spec.std_err_file.as_deref(), Some("/var/log/etl.err"));
            assert!(spec.alarm_if_fail);
            assert_eq!(spec.n_retrys, 3);
            assert_eq!(spec.term_run_time, Some(60));
            assert!(spec.condition.as_deref().unwrap().contains("upstream_job"));
        }
        _ => panic!("expected Insert"),
    }
}

#[test]
fn box_job_parsed() {
    let blocks = parse(SAMPLE_JIL).unwrap();
    match &blocks[1] {
        JilBlock::Insert(spec) => {
            assert_eq!(spec.name, "nightly_etl_box");
            assert_eq!(spec.job_type, JilJobType::Box);
        }
        _ => panic!("expected Insert for box"),
    }
}

#[test]
fn update_job_parsed() {
    let blocks = parse(SAMPLE_JIL).unwrap();
    match &blocks[2] {
        JilBlock::Update(name, partial) => {
            assert_eq!(name, "nightly_etl");
            assert_eq!(partial.alarm_if_fail, Some(false));
        }
        _ => panic!("expected Update"),
    }
}

#[test]
fn delete_job_parsed() {
    let blocks = parse(SAMPLE_JIL).unwrap();
    match &blocks[3] {
        JilBlock::Delete(name) => assert_eq!(name, "old_job"),
        _ => panic!("expected Delete"),
    }
}

#[test]
fn two_inserts_no_separator() {
    let jil = r#"
insert_job: job_a   job_type: c
command: /bin/a
insert_job: job_b   job_type: c
command: /bin/b
"#;
    let blocks = parse(jil).unwrap();
    assert_eq!(blocks.len(), 2);
}

#[test]
fn comments_ignored() {
    let jil = "/* this is a comment */\ninsert_job: x   job_type: c\ncommand: /bin/x\n";
    let blocks = parse(jil).unwrap();
    assert_eq!(blocks.len(), 1);
}

#[test]
fn unknown_attribute_produces_warning() {
    let jil = "insert_job: x   job_type: c\ncommand: /bin/x\nsome_future_attr: value\n";
    let blocks = parse(jil).unwrap();
    match &blocks[0] {
        JilBlock::Insert(spec) => {
            assert!(!spec.warnings.is_empty());
        }
        _ => panic!("expected Insert"),
    }
}

#[test]
fn bad_verb_errors() {
    let jil = "foo_job: x\n";
    let err = parse(jil).unwrap_err();
    assert!(matches!(err, JilError::UnknownVerb(_, _)));
}

#[test]
fn unterminated_comment_errors() {
    let jil = "/* not closed\ninsert_job: x   job_type: c\n";
    let err = parse(jil).unwrap_err();
    assert_eq!(err, JilError::UnterminatedComment);
}

#[test]
fn cron_from_days_and_times() {
    use rsched_core::Trigger;

    let jil = "insert_job: sched_job   job_type: c\ncommand: /bin/x\ndays_of_week: mo,fr\nstart_times: \"03:30\"\n";
    let blocks = parse(jil).unwrap();
    match &blocks[0] {
        JilBlock::Insert(spec) => {
            let out = spec.clone().into_job().unwrap();
            match &out.job.trigger {
                Trigger::Cron { expr, .. } => {
                    // Expect "30 3 * * 1,5"
                    assert!(expr.contains("30"), "expr={expr}");
                    assert!(expr.contains('3'), "expr={expr}");
                }
                other => panic!("expected cron, got {other:?}"),
            }
        }
        _ => panic!("expected Insert"),
    }
}

#[test]
fn timeout_translated_to_secs() {
    let jil = "insert_job: t   job_type: c\ncommand: /x\nterm_run_time: 2\n";
    let blocks = parse(jil).unwrap();
    match &blocks[0] {
        JilBlock::Insert(spec) => {
            let out = spec.clone().into_job().unwrap();
            assert_eq!(out.job.timeout_secs, 120);
        }
        _ => panic!(),
    }
}

#[test]
fn n_retrys_maps_to_max_attempts() {
    let jil = "insert_job: r   job_type: c\ncommand: /x\nn_retrys: 3\n";
    let blocks = parse(jil).unwrap();
    match &blocks[0] {
        JilBlock::Insert(spec) => {
            let out = spec.clone().into_job().unwrap();
            assert_eq!(out.job.retry.max_attempts, 4);
        }
        _ => panic!(),
    }
}

#[test]
fn alarm_if_fail_sets_alert_config() {
    use rsched_core::AlertEvent;
    let jil = "insert_job: a   job_type: c\ncommand: /x\nalarm_if_fail: y\n";
    let blocks = parse(jil).unwrap();
    match &blocks[0] {
        JilBlock::Insert(spec) => {
            let out = spec.clone().into_job().unwrap();
            assert!(out.job.alerts.events.contains(&AlertEvent::OnFailure));
        }
        _ => panic!(),
    }
}

#[test]
fn condition_emits_warning() {
    let jil = "insert_job: c   job_type: c\ncommand: /x\ncondition: success(dep1)\n";
    let blocks = parse(jil).unwrap();
    match &blocks[0] {
        JilBlock::Insert(spec) => {
            let out = spec.clone().into_job().unwrap();
            assert!(out.warnings.iter().any(|w| w.contains("M19")));
        }
        _ => panic!(),
    }
}

#[test]
fn file_watcher_job_type() {
    let jil = "insert_job: fw_job   job_type: fw\ncommand: /watch/path\n";
    let blocks = parse(jil).unwrap();
    match &blocks[0] {
        JilBlock::Insert(spec) => {
            assert_eq!(spec.job_type, JilJobType::FileWatcher);
        }
        _ => panic!(),
    }
}
