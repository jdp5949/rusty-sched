# Condition DSL

A condition expression is a small boolean language adapted from Autosys.
It powers both the `Condition` trigger and the `box_success` / `box_failure`
rules on a [Box](./boxes.md).

## Grammar

```text
expr        = or_expr
or_expr     = and_expr ('or' and_expr)*
and_expr    = not_expr ('and' not_expr)*
not_expr    = 'not' not_expr | atom
atom        = '(' expr ')' | func_call
func_call   = IDENT '(' job_name [ ',' lookback ] ')' [ cmp_op INT ]
            | 'value' '(' name ')'
lookback    = HH '.' MM
cmp_op      = '=' | '!=' | '<' | '<=' | '>' | '>='
```

Keywords `and` / `or` / `not` are case-insensitive. The look-back operand
uses Autosys-style `HH.MM` (hours.minutes, dot separator) and applies only
to `success` / `failure` / `done` / `numrun` / `numsuc` / `numfail`.

## Functions

| Function | Aliases | Returns | Look-back |
|---|---|---|---|
| `success(j)` | `s(j)` | Last run succeeded | yes |
| `failure(j)` | `f(j)` | Last run failed | yes |
| `done(j)` | `d(j)` | Last run is terminal | yes |
| `running(j)` | `r(j)` | Job currently running | no |
| `notrunning(j)` | `n(j)` | Inverse of `running` | no |
| `exitcode(j) <op> N` | — | Last exit code comparison | no |
| `numrun(j) <op> N` | — | Run count comparison | yes |
| `numsuc(j) <op> N` | — | Success count comparison | yes |
| `numfail(j) <op> N` | — | Failure count comparison | yes |
| `value(name)` | — | Global variable truthy lookup | n/a |

## Examples

```text
success(prep)
success(prep, 01.30) and notrunning(other)
failure(jobA) or done(jobB, 00.30)
exitcode(j) != 0
numsuc(nightly, 24.00) >= 3
(success(a) or success(b)) and value(IS_RUN_DAY)
```

## Look-back semantics

The scheduler caches up to **200 recent runs per job** in a per-tick
snapshot. A look-back of `HH.MM` filters that history by `started_at >= now -
window`. Counts are exact within the cache window; if a job runs more than
200 times within the window, older runs are truncated.

## `value(name)` truthy rule

Resolved from the `globals` table loaded once per tick. The string is
treated as truthy if it matches (case-insensitive) `y` / `yes` / `true` /
`1`; everything else is false. Unknown names return `None` (the expression
stays unresolved, no fire).

## Setting globals

```bash
rusty-sched cli global set IS_RUN_DAY y
rusty-sched cli global set REGION us-east-1
rusty-sched cli global list
rusty-sched cli global delete IS_RUN_DAY
```

Or via REST:

```bash
curl -sX POST -H "Authorization: Bearer $RSCHED_TOKEN" \
     -H "content-type: application/json" \
     -d '{"name":"IS_RUN_DAY","value":"y"}' \
     http://localhost:8080/api/v1/globals
```

## Errors

- `ParseError::Unexpected` — bad token or trailing input.
- `ParseError::UnknownFunction` — unrecognized function name.
- `ParseError::BadLookback` — malformed `HH.MM` (minutes ≥ 60).
- Look-back on `running` / `notrunning` / `exitcode` / `value` is rejected.
